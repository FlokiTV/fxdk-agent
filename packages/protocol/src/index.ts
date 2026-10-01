export type LauncherState = 'starting' | 'ready' | 'stopping' | 'error';
export type ServerState = 'stopped' | 'starting' | 'online' | 'stopping' | 'crashed';
export type ClientState =
  | 'stopped'
  | 'starting'
  | 'connecting'
  | 'active'
  | 'stopping'
  | 'crashed';
export type AgentState = 'disabled' | 'starting' | 'ready' | 'error';

export interface ProcessStatus {
  pid: number | null;
}

export interface LauncherStatus extends ProcessStatus {
  state: LauncherState;
}

export interface ServerStatus extends ProcessStatus {
  state: ServerState;
  address: string | null;
}

export interface ClientStatus extends ProcessStatus {
  id: number;
  state: ClientState;
}

export interface AgentStatus {
  enabled: boolean;
  state: AgentState;
}

export interface ControlStatus {
  launcher: LauncherStatus;
  server: ServerStatus;
  clients: ClientStatus[];
  agent: AgentStatus;
}

export interface ApiErrorDetail {
  code: string;
  message: string;
  detail?: Record<string, unknown>;
}

export interface ApiErrorResponse {
  ok: false;
  error: ApiErrorDetail;
}
