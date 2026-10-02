const SEND_SDK_MESSAGE_TO_BACKEND = '0xD651CF33';
const RESOURCE_NAME = GetCurrentResourceName();

const BASE_GAME_CAPABILITIES = [
  'game.player',
  'game.resources',
  'game.entities.nearby',
];
const SCREENSHOT_RESOURCE_NAME = 'fxdk-agent-screenshot';

const SCREENSHOT_TIMEOUT_MS = 8000;
const MAX_SCREENSHOT_DATA_URI_BYTES = 32 * 1024 * 1024;
const SDK_MESSAGE_CHUNK_CHARS = 24 * 1024;
const MAX_SDK_MESSAGE_CHARS = 36 * 1024 * 1024;
const MAX_SDK_MESSAGE_CHUNKS = 2048;

let sdkTransferCounter = 0;

const invokeSdkBackend = (message) => {
  Citizen.invokeNative(
    SEND_SDK_MESSAGE_TO_BACKEND,
    JSON.stringify(message),
  );
};

const sendSdkMessage = (type, data) => {
  const serialized = JSON.stringify({ type, data });

  if (serialized.length <= SDK_MESSAGE_CHUNK_CHARS) {
    invokeSdkBackend({ type, data });
    return;
  }

  if (serialized.length > MAX_SDK_MESSAGE_CHARS) {
    throw agentFailure(
      'AGENT_SDK_MESSAGE_TOO_LARGE',
      'Agent SDK message exceeds the bounded transfer size',
      {
        chars: serialized.length,
        maxChars: MAX_SDK_MESSAGE_CHARS,
      },
    );
  }

  const totalChunks = Math.ceil(serialized.length / SDK_MESSAGE_CHUNK_CHARS);
  if (totalChunks > MAX_SDK_MESSAGE_CHUNKS) {
    throw agentFailure(
      'AGENT_SDK_MESSAGE_TOO_MANY_CHUNKS',
      'Agent SDK message requires too many chunks',
      {
        totalChunks,
        maxChunks: MAX_SDK_MESSAGE_CHUNKS,
      },
    );
  }

  sdkTransferCounter += 1;
  const transferId = [
    RESOURCE_NAME,
    Date.now().toString(36),
    sdkTransferCounter.toString(36),
  ].join('-');

  invokeSdkBackend({
    type: 'fxdk-agent:chunk-begin',
    data: {
      transferId,
      totalChunks,
      totalLength: serialized.length,
    },
  });

  for (let index = 0; index < totalChunks; index += 1) {
    const start = index * SDK_MESSAGE_CHUNK_CHARS;
    invokeSdkBackend({
      type: 'fxdk-agent:chunk',
      data: {
        transferId,
        index,
        payload: serialized.slice(start, start + SDK_MESSAGE_CHUNK_CHARS),
      },
    });
  }

  invokeSdkBackend({
    type: 'fxdk-agent:chunk-end',
    data: { transferId },
  });
};

const agentFailure = (code, message, detail) => {
  const error = new Error(message);
  error.agentCode = code;
  error.agentDetail = detail;
  return error;
};

const finiteNumber = (value, fallback = 0) => {
  const number = Number(value);
  return Number.isFinite(number) ? number : fallback;
};

const integerInRange = (value, fallback, min, max) => {
  const number = Math.trunc(finiteNumber(value, fallback));
  return Math.min(max, Math.max(min, number));
};

const numberInRange = (value, fallback, min, max) => {
  const number = finiteNumber(value, fallback);
  return Math.min(max, Math.max(min, number));
};

const vector3 = (value) => {
  if (Array.isArray(value) || ArrayBuffer.isView(value)) {
    return {
      x: finiteNumber(value[0]),
      y: finiteNumber(value[1]),
      z: finiteNumber(value[2]),
    };
  }

  if (value && typeof value === 'object') {
    return {
      x: finiteNumber(value.x ?? value[0]),
      y: finiteNumber(value.y ?? value[1]),
      z: finiteNumber(value.z ?? value[2]),
    };
  }

  return { x: 0, y: 0, z: 0 };
};

const distance3d = (left, right) => {
  const dx = left.x - right.x;
  const dy = left.y - right.y;
  const dz = left.z - right.z;
  return Math.sqrt((dx * dx) + (dy * dy) + (dz * dz));
};

const entityBaseSnapshot = (entity, type, origin) => {
  if (!entity || !DoesEntityExist(entity)) return null;

  const coords = vector3(GetEntityCoords(entity, false));
  const snapshot = {
    entity: finiteNumber(entity),
    type,
    model: finiteNumber(GetEntityModel(entity)),
    coords,
    heading: finiteNumber(GetEntityHeading(entity)),
  };

  if (origin) {
    snapshot.distance = distance3d(origin, coords);
  }

  return snapshot;
};

const playerSnapshot = () => {
  const playerId = finiteNumber(PlayerId());
  const ped = finiteNumber(PlayerPedId());

  if (!ped || !DoesEntityExist(ped)) {
    throw agentFailure(
      'AGENT_PLAYER_UNAVAILABLE',
      'local player ped is not available',
      { playerId, ped },
    );
  }

  const vehicle = finiteNumber(GetVehiclePedIsIn(ped, false));
  const vehicleBase = vehicle && DoesEntityExist(vehicle)
    ? entityBaseSnapshot(vehicle, 'vehicle')
    : null;

  return {
    player: {
      playerId,
      serverId: finiteNumber(GetPlayerServerId(playerId)),
      ped,
      coords: vector3(GetEntityCoords(ped, false)),
      heading: finiteNumber(GetEntityHeading(ped)),
      health: finiteNumber(GetEntityHealth(ped)),
      maxHealth: finiteNumber(GetEntityMaxHealth(ped)),
      armor: finiteNumber(GetPedArmour(ped)),
      dead: Boolean(IsEntityDead(ped)),
      model: finiteNumber(GetEntityModel(ped)),
      vehicle: vehicleBase
        ? {
            ...vehicleBase,
            isDriver: finiteNumber(GetPedInVehicleSeat(vehicle, -1)) === ped,
          }
        : null,
    },
  };
};

const resourcesSnapshot = (params = {}) => {
  const limit = integerInRange(params.limit, 128, 1, 512);
  const total = Math.max(0, Math.trunc(finiteNumber(GetNumResources())));
  const resources = [];

  for (let index = 0; index < total && resources.length < limit; index += 1) {
    const name = GetResourceByFindIndex(index);
    if (typeof name !== 'string' || name.length === 0) continue;

    resources.push({
      name,
      state: String(GetResourceState(name) || 'unknown'),
    });
  }

  return {
    total,
    returned: resources.length,
    truncated: resources.length < total,
    resources,
  };
};

const nearbyEntitiesSnapshot = (params = {}) => {
  const radius = numberInRange(params.radius, 50, 1, 500);
  const limit = integerInRange(params.limit, 64, 1, 256);
  const requestedTypes = Array.isArray(params.types)
    ? params.types.map((value) => String(value).toLowerCase())
    : ['ped', 'vehicle', 'object'];

  const allowedTypes = new Set(['ped', 'vehicle', 'object']);
  const types = [...new Set(requestedTypes.filter((type) => allowedTypes.has(type)))];

  if (types.length === 0) {
    throw agentFailure(
      'AGENT_ENTITY_TYPES_INVALID',
      'types must include at least one of ped, vehicle, or object',
      { types: requestedTypes },
    );
  }

  const playerPed = finiteNumber(PlayerPedId());
  if (!playerPed || !DoesEntityExist(playerPed)) {
    throw agentFailure(
      'AGENT_PLAYER_UNAVAILABLE',
      'local player ped is not available',
      { playerPed },
    );
  }

  const origin = vector3(GetEntityCoords(playerPed, false));
  const poolByType = {
    ped: 'CPed',
    vehicle: 'CVehicle',
    object: 'CObject',
  };
  const entities = [];

  for (const type of types) {
    const pool = GetGamePool(poolByType[type]);
    if (!Array.isArray(pool) && !ArrayBuffer.isView(pool)) continue;

    for (const rawEntity of pool) {
      const entity = finiteNumber(rawEntity);
      const snapshot = entityBaseSnapshot(entity, type, origin);
      if (!snapshot || snapshot.distance > radius) continue;

      if (type === 'ped') {
        snapshot.isPlayer = Boolean(IsPedAPlayer(entity));
      }

      entities.push(snapshot);
    }
  }

  entities.sort((left, right) => left.distance - right.distance);
  const totalWithinRadius = entities.length;
  const bounded = entities.slice(0, limit);

  return {
    origin,
    radius,
    limit,
    types,
    totalWithinRadius,
    returned: bounded.length,
    truncated: totalWithinRadius > bounded.length,
    entities: bounded,
  };
};

const screenshotSnapshot = (params = {}) => new Promise((resolve, reject) => {
  const quality = numberInRange(params.quality, 0.92, 0.1, 1);
  let settled = false;

  const timer = setTimeout(() => {
    if (settled) return;
    settled = true;
    reject(agentFailure(
      'AGENT_SCREENSHOT_TIMEOUT',
      'game screenshot capture timed out',
      { timeoutMs: SCREENSHOT_TIMEOUT_MS },
    ));
  }, SCREENSHOT_TIMEOUT_MS);

  const finish = (callback) => {
    if (settled) return;
    settled = true;
    clearTimeout(timer);
    callback();
  };

  try {
    if (GetResourceState(SCREENSHOT_RESOURCE_NAME) !== 'started') {
      finish(() => reject(agentFailure(
        'AGENT_SCREENSHOT_UNAVAILABLE',
        'screenshot resource is not started',
        { resource: SCREENSHOT_RESOURCE_NAME },
      )));
      return;
    }

    const resourceExports = global.exports?.[SCREENSHOT_RESOURCE_NAME];
    const capture = resourceExports?.requestScreenshot;

    if (typeof capture !== 'function') {
      finish(() => reject(agentFailure(
        'AGENT_SCREENSHOT_UNAVAILABLE',
        'screenshot capture export is unavailable',
        { resource: RESOURCE_NAME },
      )));
      return;
    }

    capture(
      {
        encoding: 'png',
        quality,
      },
      (dataUri) => {
        finish(() => {
          if (
            typeof dataUri !== 'string'
            || !dataUri.startsWith('data:image/png;base64,')
          ) {
            reject(agentFailure(
              'AGENT_SCREENSHOT_INVALID',
              'screenshot capture returned an invalid PNG data URI',
            ));
            return;
          }

          if (dataUri.length > MAX_SCREENSHOT_DATA_URI_BYTES) {
            reject(agentFailure(
              'AGENT_SCREENSHOT_TOO_LARGE',
              'screenshot exceeds the bounded payload size',
              {
                bytes: dataUri.length,
                maxBytes: MAX_SCREENSHOT_DATA_URI_BYTES,
              },
            ));
            return;
          }

          resolve({
            dataUri,
            mimeType: 'image/png',
            encoding: 'png',
            capturedAt: new Date().toISOString(),
          });
        });
      },
    );
  } catch (error) {
    finish(() => reject(agentFailure(
      'AGENT_SCREENSHOT_FAILED',
      error instanceof Error ? error.message : String(error),
    )));
  }
});

const executeRequest = async (request) => {
  switch (request.method) {
    case 'game.player':
      return playerSnapshot();
    case 'game.resources':
      return resourcesSnapshot(request.params || {});
    case 'game.entities.nearby':
      return nearbyEntitiesSnapshot(request.params || {});
    case 'game.screenshot':
      return screenshotSnapshot(request.params || {});
    default:
      throw agentFailure(
        'AGENT_METHOD_UNSUPPORTED',
        'unsupported game method: ' + String(request.method),
        { method: request.method },
      );
  }
};

on('fxdk-agent:request', async (rawRequest) => {
  let request;

  try {
    request = typeof rawRequest === 'string'
      ? JSON.parse(rawRequest)
      : rawRequest;

    if (!request || typeof request.requestId !== 'string') {
      return;
    }

    const result = await executeRequest(request);
    sendSdkMessage('fxdk-agent:response', {
      requestId: request.requestId,
      ok: true,
      result,
    });
  } catch (error) {
    if (!request || typeof request.requestId !== 'string') {
      return;
    }

    sendSdkMessage('fxdk-agent:response', {
      requestId: request.requestId,
      ok: false,
      error: {
        code: error?.agentCode || 'AGENT_GAME_EXECUTION_FAILED',
        message: error instanceof Error ? error.message : String(error),
        detail: error?.agentDetail,
      },
    });
  }
});

const currentCapabilities = () => {
  const capabilities = [...BASE_GAME_CAPABILITIES];

  if (GetResourceState(SCREENSHOT_RESOURCE_NAME) === 'started') {
    capabilities.push('game.screenshot');
  }

  return capabilities;
};

const announceReady = () => {
  sendSdkMessage('fxdk-agent:ready', {
    resource: RESOURCE_NAME,
    capabilities: currentCapabilities(),
  });
};

on('onClientResourceStart', (resourceName) => {
  if (resourceName === RESOURCE_NAME) {
    announceReady();
  }
});

on('onClientResourceStop', (resourceName) => {
  if (resourceName === RESOURCE_NAME) {
    sendSdkMessage('fxdk-agent:stopped', { resource: RESOURCE_NAME });
  }
});

setInterval(announceReady, 5000);
