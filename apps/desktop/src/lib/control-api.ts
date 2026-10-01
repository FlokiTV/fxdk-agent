import type {
  ApiErrorResponse,
  AppConfig,
  ConfigPatch,
  ControlStatus,
  SessionStartRequest,
} from '@fxdk-agent/protocol';

export const CONTROL_API_BASE_URL = 'http://127.0.0.1:35418';

export class ControlApiError extends Error {
  readonly status: number;
  readonly payload: ApiErrorResponse | null;

  constructor(
    message: string,
    status: number,
    payload: ApiErrorResponse | null = null,
  ) {
    super(message);
    this.name = 'ControlApiError';
    this.status = status;
    this.payload = payload;
  }
}

async function requestJson<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  const response = await fetch(`${CONTROL_API_BASE_URL}${path}`, {
    ...init,
    headers: {
      Accept: 'application/json',
      ...(init?.body ? { 'Content-Type': 'application/json' } : {}),
      ...init?.headers,
    },
  });

  if (!response.ok) {
    let payload: ApiErrorResponse | null = null;
    try {
      payload = (await response.json()) as ApiErrorResponse;
    } catch {
      // Non-JSON errors are surfaced with the HTTP status text.
    }

    throw new ControlApiError(
      payload?.error.message ?? response.statusText ?? 'Control API request failed',
      response.status,
      payload,
    );
  }

  return (await response.json()) as T;
}

export function getStatus(): Promise<ControlStatus> {
  return requestJson<ControlStatus>('/v1/status');
}

export function startSession(
  request: SessionStartRequest = { clients: 1 },
): Promise<ControlStatus> {
  return requestJson<ControlStatus>('/v1/session/start', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

export function stopSession(): Promise<ControlStatus> {
  return requestJson<ControlStatus>('/v1/session/stop', {
    method: 'POST',
  });
}

export function getConfig(): Promise<AppConfig> {
  return requestJson<AppConfig>('/v1/config');
}

export function patchConfig(patch: ConfigPatch): Promise<AppConfig> {
  return requestJson<AppConfig>('/v1/config', {
    method: 'PATCH',
    body: JSON.stringify(patch),
  });
}
