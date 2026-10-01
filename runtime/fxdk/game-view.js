(() => {
  'use strict';

  const listeners = new Map();
  const events = {
    on(name, listener) {
      const group = listeners.get(name) || new Set();
      group.add(listener);
      listeners.set(name, group);
      return () => group.delete(listener);
    },
    async emit(name, data) {
      for (const listener of listeners.get(name) || []) {
        await listener(data);
      }
    },
    async emitByObject(payload) {
      if (payload && typeof payload.type === 'string') {
        await this.emit(payload.type, payload.data);
      }
    },
  };

  if (!window.shellApi) {
    window.shellApi = { events, commands: {} };
  } else if (!window.shellApi.events) {
    window.shellApi.events = events;
  }

  const params = new URLSearchParams(window.location.search);
  const clientNumber = params.get('client') || '1';
  const serverAddress = params.get('server') || '127.0.0.1:30120';
  document.title = `FXDK Agent - Client ${clientNumber}`;

  const canvas = document.getElementById('game');
  const status = document.getElementById('status');
  const title = document.getElementById('status-title');
  const detail = document.getElementById('status-detail');

  const native = (name, ...args) => {
    const fn = window[name];
    if (typeof fn !== 'function') return false;
    const result = fn(...args);
    return result === undefined ? true : result;
  };

  const setStatus = (heading, message, active = false) => {
    title.textContent = heading;
    detail.textContent = message;
    status.classList.toggle('active', active);
  };

  window.addEventListener('message', (event) => {
    if (event.data?.type !== 'sdkApiMessage') return;

    let payload;
    try {
      payload = typeof event.data.data === 'string'
        ? JSON.parse(event.data.data)
        : event.data.data;
    } catch {
      return;
    }

    if (payload?.type === 'session-bootstrap') {
      setStatus('Starting GameRuntime', `Target: ${serverAddress}`);
    } else if (payload?.type === 'session-runtime-ready') {
      setStatus('GameRuntime ready', 'Connecting to the local server…');
    } else if (payload?.type === 'session-connecting') {
      setStatus('Connecting', `Target: ${serverAddress}`);
    } else if (payload?.type === 'session-connection-state') {
      const current = Number(payload.current) || 0;
      setStatus(
        current === 8 ? 'Session active' : 'Connecting',
        current === 8 ? `Connected to ${serverAddress}` : `Connection state ${current}/8`,
        current === 8,
      );
    } else if (payload?.type === 'session-active') {
      setStatus('Session active', `Connected to ${serverAddress}`, true);
      setTimeout(() => status.remove(), 1200);
    }
  });

  const gl = canvas.getContext('webgl', {
    antialias: false,
    depth: false,
    alpha: false,
    stencil: false,
    desynchronized: true,
    powerPreference: 'high-performance',
  });

  if (!gl) {
    setStatus('GameView error', 'WebGL is unavailable.');
    return;
  }

  const compileShader = (type, source) => {
    const shader = gl.createShader(type);
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      throw new Error(gl.getShaderInfoLog(shader) || 'shader compilation failed');
    }
    return shader;
  };

  const program = gl.createProgram();
  gl.attachShader(program, compileShader(gl.VERTEX_SHADER, `
    attribute vec2 a_position;
    attribute vec2 a_texcoord;
    varying vec2 textureCoordinate;
    void main() {
      gl_Position = vec4(a_position, 0.0, 1.0);
      textureCoordinate = a_texcoord;
    }
  `));
  gl.attachShader(program, compileShader(gl.FRAGMENT_SHADER, `
    varying highp vec2 textureCoordinate;
    uniform sampler2D external_texture;
    void main() {
      gl_FragColor = texture2D(external_texture, textureCoordinate);
    }
  `));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(program) || 'shader link failed');
  }
  gl.useProgram(program);

  const bindAttribute = (name, values) => {
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(values), gl.STATIC_DRAW);
    const location = gl.getAttribLocation(program, name);
    gl.enableVertexAttribArray(location);
    gl.vertexAttribPointer(location, 2, gl.FLOAT, false, 0, 0);
  };

  bindAttribute('a_position', [-1, -1, 1, -1, -1, 1, 1, 1]);
  bindAttribute('a_texcoord', [0, 1, 1, 1, 0, 0, 1, 0]);

  const texture = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.texImage2D(
    gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE,
    new Uint8Array([0, 0, 255, 255]),
  );
  gl.texParameterf(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.texParameterf(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameterf(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameterf(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  gl.uniform1i(gl.getUniformLocation(program, 'external_texture'), 0);

  const resize = () => {
    const width = Math.max(1, Math.floor(canvas.clientWidth));
    const height = Math.max(1, Math.floor(canvas.clientHeight));
    native('resizeGame', width, height);
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
      gl.viewport(0, 0, width, height);
    }
  };

  const render = () => {
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    gl.finish();
    requestAnimationFrame(render);
  };

  const keyCode = (event) => event.which || event.keyCode;
  window.addEventListener('keydown', (event) => {
    native('setKeyState', keyCode(event), true);
    if (event.key.length === 1) native('setInputChar', event.key);
    event.preventDefault();
  });
  window.addEventListener('keyup', (event) => {
    native('setKeyState', keyCode(event), false);
    native('setInputChar', '\0');
    event.preventDefault();
  });
  canvas.addEventListener('mousemove', (event) => {
    if (document.pointerLockElement === canvas) {
      native('setRawMouseCapture', true);
    }
  });
  canvas.addEventListener('click', () => {
    canvas.focus();
    canvas.requestPointerLock?.();
  });
  document.addEventListener('pointerlockchange', () => {
    native('setRawMouseCapture', document.pointerLockElement === canvas);
  });
  window.addEventListener('resize', resize);

  resize();
  requestAnimationFrame(render);
})();
