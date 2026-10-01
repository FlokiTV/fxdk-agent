export type LauncherState = 'starting' | 'ready' | 'stopping' | 'error';
export type SessionState = 'stopped' | 'starting' | 'active' | 'stopping' | 'error';
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

export interface SessionStatus {
  id: string | null;
  state: SessionState;
  lastError: string | null;
}

export interface SessionStartRequest {
  clients: number;
}

export interface ServerStatus extends ProcessStatus {
  state: ServerState;
  address: string | null;
  exitCode: number | null;
  lastError: string | null;
  logTail: string[];
}

export interface ClientStatus extends ProcessStatus {
  id: number;
  state: ClientState;
  exitCode: number | null;
  lastError: string | null;
  connectionState: number | null;
  gameProcessState: number | null;
  logTail: string[];
}

export interface AgentStatus {
  enabled: boolean;
  state: AgentState;
}

export interface ControlStatus {
  launcher: LauncherStatus;
  session: SessionStatus;
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

export const CONFIG_SCHEMA_VERSION = 1 as const;

export interface SyntheticIdentityConfig {
  enabled: boolean;
}

export interface AppConfig {
  schemaVersion: number;
  serverProject: string | null;
  fxserverPath: string | null;
  fivemPath: string | null;
  serverAddress: string;
  syntheticIdentity: SyntheticIdentityConfig;
}

export interface SyntheticIdentityPatch {
  enabled?: boolean;
}

export interface ConfigPatch {
  serverProject?: string | null;
  fxserverPath?: string | null;
  fivemPath?: string | null;
  serverAddress?: string;
  syntheticIdentity?: SyntheticIdentityPatch;
}

export interface ConfigValidationIssue {
  field: 'serverProject' | 'fxserverPath' | 'fivemPath' | string;
  code: string;
  message: string;
}

export interface ConfigValidationErrorDetail {
  issues: ConfigValidationIssue[];
}
