const http = require('http');

const SERVER_ADDRESS = GetConvar('fxdk_agent_server', '127.0.0.1:30120');
const UI_URL = GetConvar('fxdk_agent_ui_url', 'http://127.0.0.1:35419/?client=1');
const CONTROL_URL = GetConvar('fxdk_agent_control_url', 'http://127.0.0.1:35418');
const CLIENT_ID = Number(GetConvar('fxdk_agent_client', '1')) || 1;

const BASE_AGENT_CAPABILITIES = [
  'runtime.ping',
  'runtime.status',
];
const SUPPORTED_GAME_CAPABILITIES = new Set([
  'game.player',
  'game.resources',
  'game.entities.nearby',
  'game.screenshot',
]);
const GAME_REQUEST_TIMEOUT_MS = 3500;
const SCREENSHOT_REQUEST_TIMEOUT_MS = 10000;
const MAX_SDK_TRANSFER_CHARS = 36 * 1024 * 1024;
const MAX_SDK_TRANSFER_CHUNKS = 2048;
const SDK_TRANSFER_TTL_MS = 15000;

let gameProcessState = 0;
let gameLaunched = false;
let connectionState = 0;
let connectionRequested = false;
let active = false;
let agentRegistered = false;
let agentPollInFlight = false;
let gameBridgeReady = false;
let gameCapabilities = [];
let latestSdkMessageType = null;

const pendingGameRequests = new Map();
const sdkChunkTransfers = new Map();

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
      path: url.pathname + url.search,
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

const agentFailure = (code, message, detail) => {
  const error = new Error(message);
  error.agentCode = code;
  error.agentDetail = detail;
  return error;
};

const runtimeContext = () => ({
  clientId: CLIENT_ID,
  serverAddress: SERVER_ADDRESS,
  gameProcessState,
  connectionState,
  active,
  timestamp: new Date().toISOString(),
});

const requireGameReady = () => {
  if (!active || gameProcessState !== 2) {
    throw agentFailure(
      'AGENT_GAME_NOT_READY',
      'game state is unavailable until the managed client is ACTIVE',
      { gameProcessState, connectionState, active },
    );
  }

  if (!gameBridgeReady) {
    throw agentFailure(
      'AGENT_GAME_BRIDGE_UNAVAILABLE',
      'the managed in-game resource has not reported ready',
      { gameBridgeReady },
    );
  }
};

const advertisedCapabilities = () => [
  ...BASE_AGENT_CAPABILITIES,
  ...(gameBridgeReady ? gameCapabilities : []),
];

const resetGameBridge = (code = 'AGENT_GAME_BRIDGE_RESET') => {
  gameBridgeReady = false;
  gameCapabilities = [];
  agentRegistered = false;

  for (const pending of pendingGameRequests.values()) {
    clearTimeout(pending.timer);
    pending.reject(agentFailure(
      code,
      'in-game Agent bridge was reset before the request completed',
    ));
  }

  pendingGameRequests.clear();
};

const handleSdkChunk = (message) => {
  const chunkTypes = new Set([
    'fxdk-agent:chunk-begin',
    'fxdk-agent:chunk',
    'fxdk-agent:chunk-end',
  ]);

  if (!chunkTypes.has(message.type)) return false;

  const data = message.data;
  if (!data || typeof data.transferId !== 'string') return true;

  if (message.type === 'fxdk-agent:chunk-begin') {
    const totalChunks = Number(data.totalChunks);
    const totalLength = Number(data.totalLength);

    if (
      !Number.isInteger(totalChunks)
      || totalChunks < 1
      || totalChunks > MAX_SDK_TRANSFER_CHUNKS
      || !Number.isInteger(totalLength)
      || totalLength < 1
      || totalLength > MAX_SDK_TRANSFER_CHARS
    ) {
      return true;
    }

    sdkChunkTransfers.set(data.transferId, {
      totalChunks,
      totalLength,
      chunks: new Array(totalChunks),
      receivedLength: 0,
      updatedAt: Date.now(),
    });
    return true;
  }

  if (message.type === 'fxdk-agent:chunk') {
    const transfer = sdkChunkTransfers.get(data.transferId);
    const index = Number(data.index);
    const payload = data.payload;

    if (
      !transfer
      || !Number.isInteger(index)
      || index < 0
      || index >= transfer.totalChunks
      || typeof payload !== 'string'
    ) {
      return true;
    }

    const previous = transfer.chunks[index];
    if (typeof previous === 'string') {
      transfer.receivedLength -= previous.length;
    }

    transfer.chunks[index] = payload;
    transfer.receivedLength += payload.length;
    transfer.updatedAt = Date.now();

    if (transfer.receivedLength > transfer.totalLength) {
      sdkChunkTransfers.delete(data.transferId);
    }

    return true;
  }

  if (message.type === 'fxdk-agent:chunk-end') {
    const transfer = sdkChunkTransfers.get(data.transferId);
    sdkChunkTransfers.delete(data.transferId);

    if (!transfer) return true;
    if (transfer.receivedLength !== transfer.totalLength) return true;
    if (transfer.chunks.some((chunk) => typeof chunk !== 'string')) return true;

    const serialized = transfer.chunks.join('');
    if (serialized.length !== transfer.totalLength) return true;

    try {
      handleSdkMessage(JSON.parse(serialized));
    } catch {
      latestSdkMessageType = 'invalid-chunked-json';
    }
    return true;
  }

  return false;
};

const handleSdkMessage = (rawMessage) => {
  let message;

  try {
    message = typeof rawMessage === 'string'
      ? JSON.parse(rawMessage)
      : rawMessage;
  } catch {
    latestSdkMessageType = 'invalid-json';
    return;
  }

  if (!message || typeof message.type !== 'string') return;

  if (handleSdkChunk(message)) {
    latestSdkMessageType = message.type;
    return;
  }

  latestSdkMessageType = message.type;

  if (message.type === 'fxdk-agent:ready') {
    const announced = Array.isArray(message.data?.capabilities)
      ? message.data.capabilities
      : [];

    gameCapabilities = [...new Set(
      announced
        .map((capability) => String(capability))
        .filter((capability) => SUPPORTED_GAME_CAPABILITIES.has(capability)),
    )].sort();

    gameBridgeReady = true;
    agentRegistered = false;
    registerAgentRuntime();
    return;
  }

  if (message.type === 'fxdk-agent:stopped') {
    resetGameBridge();
    return;
  }

  if (message.type !== 'fxdk-agent:response') return;

  const response = message.data;
  if (!response || typeof response.requestId !== 'string') return;

  const pending = pendingGameRequests.get(response.requestId);
  if (!pending) return;

  pendingGameRequests.delete(response.requestId);
  clearTimeout(pending.timer);

  if (response.ok) {
    const result = response.result && typeof response.result === 'object'
      ? { ...response.result, context: runtimeContext() }
      : { value: response.result, context: runtimeContext() };
    pending.resolve(result);
    return;
  }

  pending.reject(agentFailure(
    response.error?.code || 'AGENT_GAME_EXECUTION_FAILED',
    response.error?.message || 'in-game Agent request failed',
    response.error?.detail,
  ));
};

const executeGameRequest = (request) => {
  requireGameReady();

  if (!gameCapabilities.includes(request.method)) {
    throw agentFailure(
      'AGENT_METHOD_UNSUPPORTED',
      'in-game method is not advertised by the current runtime: ' + String(request.method),
      { method: request.method, capabilities: gameCapabilities },
    );
  }

  return new Promise((resolve, reject) => {
    const timeoutMs = request.method === 'game.screenshot'
      ? SCREENSHOT_REQUEST_TIMEOUT_MS
      : GAME_REQUEST_TIMEOUT_MS;
    const timer = setTimeout(() => {
      pendingGameRequests.delete(request.requestId);
      reject(agentFailure(
        'AGENT_GAME_REQUEST_TIMEOUT',
        'in-game Agent request timed out',
        { requestId: request.requestId, method: request.method, timeoutMs },
      ));
    }, timeoutMs);

    pendingGameRequests.set(request.requestId, { resolve, reject, timer });

    try {
      emit(
        'sdk:sendGameClientEvent',
        'fxdk-agent:request',
        JSON.stringify({
          requestId: request.requestId,
          method: request.method,
          params: request.params || {},
        }),
      );
    } catch (error) {
      clearTimeout(timer);
      pendingGameRequests.delete(request.requestId);
      reject(agentFailure(
        'AGENT_GAME_BRIDGE_SEND_FAILED',
        error instanceof Error ? error.message : String(error),
      ));
    }
  });
};

const registerAgentRuntime = async () => {
  try {
    const response = await requestJson('POST', '/v1/agent/runtime/register', {
      clientId: CLIENT_ID,
      capabilities: advertisedCapabilities(),
    });
    agentRegistered = response.statusCode === 200;
  } catch {
    agentRegistered = false;
  }
};

const executeAgentRequest = async (request) => {
  if (!request || typeof request.requestId !== 'string') return;

  let response;

  try {
    let result;

    if (request.method === 'runtime.ping') {
      result = {
        pong: true,
        clientId: CLIENT_ID,
        timestamp: new Date().toISOString(),
      };
    } else if (request.method === 'runtime.status') {
      result = {
        clientId: CLIENT_ID,
        serverAddress: SERVER_ADDRESS,
        gameProcessState,
        connectionState,
        gameLaunched,
        connectionRequested,
        active,
        gameBridgeReady,
        gameCapabilities,
        latestSdkMessageType,
      };
    } else if (SUPPORTED_GAME_CAPABILITIES.has(request.method)) {
      result = await executeGameRequest(request);
    } else {
      throw agentFailure(
        'AGENT_METHOD_UNSUPPORTED',
        'unsupported runtime method: ' + String(request.method),
        { method: request.method },
      );
    }

    response = {
      requestId: request.requestId,
      clientId: CLIENT_ID,
      ok: true,
      result,
    };
  } catch (error) {
    response = {
      requestId: request.requestId,
      clientId: CLIENT_ID,
      ok: false,
      error: {
        code: error?.agentCode || 'AGENT_RUNTIME_EXECUTION_FAILED',
        message: error instanceof Error ? error.message : String(error),
        detail: error?.agentDetail,
      },
    };
  }

  try {
    const responseTimeoutMs = request.method === 'game.screenshot' ? 10000 : 2000;
    const result = await requestJson(
      'POST',
      '/v1/agent/runtime/respond',
      response,
      responseTimeoutMs,
    );
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
      '/v1/agent/runtime/next?clientId=' + CLIENT_ID,
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
    resetGameBridge();
  }

  postEvent('process-state', {
    current: gameProcessState,
    previous: Number(previous) || 0,
  });
});

on('sdk:backendMessage', (message) => {
  handleSdkMessage(message);
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
    resetGameBridge();
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

setInterval(() => {
  const cutoff = Date.now() - SDK_TRANSFER_TTL_MS;
  for (const [transferId, transfer] of sdkChunkTransfers.entries()) {
    if (transfer.updatedAt < cutoff) {
      sdkChunkTransfers.delete(transferId);
    }
  }
}, 5000);
