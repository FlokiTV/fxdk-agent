const http = require('http');

const SERVER_ADDRESS = GetConvar('fxdk_agent_server', '127.0.0.1:30120');
const UI_URL = GetConvar('fxdk_agent_ui_url', 'http://127.0.0.1:35419/?client=1');
const CONTROL_URL = GetConvar('fxdk_agent_control_url', 'http://127.0.0.1:35418');
const CLIENT_ID = Number(GetConvar('fxdk_agent_client', '1')) || 1;

let gameProcessState = 0;
let gameLaunched = false;
let connectionState = 0;
let connectionRequested = false;
let active = false;
let agentRegistered = false;
let agentPollInFlight = false;

const AGENT_CAPABILITIES = [
  'runtime.ping',
  'runtime.status',
];

const requestJson = (method, path, payload, timeout = 2000) => new Promise((resolve, reject) => {
  const body = payload === undefined ? null : JSON.stringify(payload);
  const url = new URL(path, CONTROL_URL);
  const headers = { accept: 'application/json' };
  if (body !== null) {
    headers['content-type'] = 'application/json';
    headers['content-length'] = Buffer.byteLength(body);
  }

  const request = http.request(
    {
      hostname: url.hostname,
      port: Number(url.port || 80),
      path: `${url.pathname}${url.search}`,
      method,
      timeout,
      headers,
    },
    (response) => {
      const chunks = [];
      response.on('data', (chunk) => chunks.push(Buffer.from(chunk)));
      response.on('end', () => {
        const raw = Buffer.concat(chunks).toString('utf8');
        if (!raw) {
          resolve({ statusCode: response.statusCode || 0, data: null });
          return;
        }

        try {
          resolve({
            statusCode: response.statusCode || 0,
            data: JSON.parse(raw),
          });
        } catch (error) {
          reject(error);
        }
      });
    },
  );

  request.on('timeout', () => request.destroy(new Error('request timeout')));
  request.on('error', reject);
  request.end(body || undefined);
});

const postEvent = (kind, data = {}) => {
  const body = JSON.stringify({
    clientId: CLIENT_ID,
    kind,
    ...data,
  });
  const url = new URL('/v1/client/events', CONTROL_URL);
  const request = http.request(
    {
      hostname: url.hostname,
      port: Number(url.port || 80),
      path: url.pathname,
      method: 'POST',
      timeout: 2000,
      headers: {
        'content-type': 'application/json',
        'content-length': Buffer.byteLength(body),
      },
    },
    (response) => response.resume(),
  );
  request.on('timeout', () => request.destroy());
  request.on('error', () => undefined);
  request.end(body);
};

const notifyBrowser = (payload) => {
  emit('sdk:api:send', JSON.stringify(payload));
};

const registerAgentRuntime = async () => {
  try {
    const response = await requestJson('POST', '/v1/agent/runtime/register', {
      clientId: CLIENT_ID,
      capabilities: AGENT_CAPABILITIES,
    });
    agentRegistered = response.statusCode === 200;
  } catch {
    agentRegistered = false;
  }
};

const executeAgentRequest = async (request) => {
  if (!request || typeof request.requestId !== 'string') {
    return;
  }

  let response;
  if (request.method === 'runtime.ping') {
    response = {
      requestId: request.requestId,
      clientId: CLIENT_ID,
      ok: true,
      result: {
        pong: true,
        clientId: CLIENT_ID,
        timestamp: new Date().toISOString(),
      },
    };
  } else if (request.method === 'runtime.status') {
    response = {
      requestId: request.requestId,
      clientId: CLIENT_ID,
      ok: true,
      result: {
        clientId: CLIENT_ID,
        serverAddress: SERVER_ADDRESS,
        gameProcessState,
        connectionState,
        gameLaunched,
        connectionRequested,
        active,
      },
    };
  } else {
    response = {
      requestId: request.requestId,
      clientId: CLIENT_ID,
      ok: false,
      error: {
        code: 'AGENT_METHOD_UNSUPPORTED',
        message: `unsupported runtime method: ${String(request.method)}`,
      },
    };
  }

  try {
    const result = await requestJson('POST', '/v1/agent/runtime/respond', response);
    if (result.statusCode === 409) {
      agentRegistered = false;
    }
  } catch {
    agentRegistered = false;
  }
};

const pollAgentRequest = async () => {
  if (agentPollInFlight) return;
  agentPollInFlight = true;

  try {
    if (!agentRegistered) {
      await registerAgentRuntime();
      if (!agentRegistered) return;
    }

    const response = await requestJson(
      'GET',
      `/v1/agent/runtime/next?clientId=${CLIENT_ID}`,
      undefined,
      1500,
    );

    if (response.statusCode === 200 && response.data) {
      await executeAgentRequest(response.data);
    } else if (response.statusCode === 409) {
      agentRegistered = false;
    }
  } catch {
    agentRegistered = false;
  } finally {
    agentPollInFlight = false;
  }
};

const connect = (reason) => {
  if (active) return;
  connectionRequested = true;
  postEvent('connect-requested', { reason, serverAddress: SERVER_ADDRESS });
  notifyBrowser({
    type: 'session-connecting',
    reason,
    serverAddress: SERVER_ADDRESS,
  });
  emit('sdk:connectClientTo', SERVER_ADDRESS);
};

on('sdk:gameLaunched', () => {
  gameLaunched = true;
  postEvent('game-launched', { gameProcessState });
  notifyBrowser({ type: 'session-runtime-ready', serverAddress: SERVER_ADDRESS });
  setTimeout(() => connect('game-launched'), 1800);
});

on('sdk:gameProcessStateChanged', (current, previous) => {
  gameProcessState = Number(current) || 0;
  if (gameProcessState === 0) {
    gameLaunched = false;
    connectionRequested = false;
    connectionState = 0;
    active = false;
  }
  postEvent('process-state', {
    current: gameProcessState,
    previous: Number(previous) || 0,
  });
});

on('sdk:connectionStateChanged', (current, previous) => {
  connectionState = Number(current) || 0;
  active = connectionState === 8;
  postEvent('connection-state', {
    current: connectionState,
    previous: Number(previous) || 0,
    active,
  });
  notifyBrowser({
    type: 'session-connection-state',
    current: connectionState,
    previous: Number(previous) || 0,
    active,
  });

  if (active) {
    notifyBrowser({ type: 'session-active', serverAddress: SERVER_ADDRESS });
  } else if (connectionState === 0 && Number(previous) > 0) {
    connectionRequested = false;
  }
});

setTimeout(() => {
  registerAgentRuntime();
  emit('sdk:openBrowser', UI_URL);
  notifyBrowser({ type: 'session-bootstrap', serverAddress: SERVER_ADDRESS });
  postEvent('sdk-ready', { serverAddress: SERVER_ADDRESS });
  emit('sdk:startGame');
}, 1200);

setTimeout(() => {
  if (!active && !connectionRequested && (gameLaunched || gameProcessState === 2)) {
    connect('runtime-fallback');
  }
}, 7000);

setInterval(() => {
  postEvent('heartbeat', {
    gameProcessState,
    connectionState,
    active,
  });
  registerAgentRuntime();
}, 5000);

setInterval(pollAgentRequest, 250);
