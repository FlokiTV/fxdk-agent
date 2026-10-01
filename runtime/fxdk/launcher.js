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
}, 5000);
