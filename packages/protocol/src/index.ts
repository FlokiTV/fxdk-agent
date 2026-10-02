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

export interface AgentRuntimeError {
  code: string;
  message: string;
  detail?: unknown;
}

export interface AgentInvokeRequest {
  clientId?: number;
  method: string;
  params?: unknown;
  timeoutMs?: number;
}

export interface AgentInvokeResponse {
  requestId: string;
  ok: boolean;
  result?: unknown;
  error?: AgentRuntimeError;
}

export interface AgentCapabilitiesResponse {
  ready: boolean;
  clientId: number | null;
  capabilities: string[];
}

export interface AgentRuntimeRegistration {
  clientId: number;
  capabilities: string[];
}

export interface AgentRuntimeRequest {
  requestId: string;
  clientId: number;
  method: string;
  params: unknown;
}

export interface AgentRuntimeResponse {
  requestId: string;
  clientId: number;
  ok: boolean;
  result?: unknown;
  error?: AgentRuntimeError;
}

export interface AgentVector3 {
  x: number;
  y: number;
  z: number;
}

export interface AgentRuntimeContext {
  clientId: number;
  serverAddress: string;
  gameProcessState: number;
  connectionState: number;
  active: boolean;
  timestamp: string;
}

export interface AgentVehicleSnapshot {
  entity: number;
  type: 'vehicle';
  model: number;
  coords: AgentVector3;
  heading: number;
  isDriver: boolean;
}

export interface AgentPlayerSnapshot {
  context: AgentRuntimeContext;
  player: {
    playerId: number;
    serverId: number;
    ped: number;
    coords: AgentVector3;
    heading: number;
    health: number;
    maxHealth: number;
    armor: number;
    dead: boolean;
    model: number;
    vehicle: AgentVehicleSnapshot | null;
  };
}

export interface AgentResourcesParams {
  limit?: number;
}

export interface AgentResourceSnapshot {
  name: string;
  state: string;
}

export interface AgentResourcesSnapshot {
  context: AgentRuntimeContext;
  total: number;
  returned: number;
  truncated: boolean;
  resources: AgentResourceSnapshot[];
}

export type AgentEntityType = 'ped' | 'vehicle' | 'object';

export interface AgentNearbyEntitiesParams {
  radius?: number;
  limit?: number;
  types?: AgentEntityType[];
}

export interface AgentEntitySnapshot {
  entity: number;
  type: AgentEntityType;
  model: number;
  coords: AgentVector3;
  heading: number;
  distance: number;
  isPlayer?: boolean;
}

export interface AgentNearbyEntitiesSnapshot {
  context: AgentRuntimeContext;
  origin: AgentVector3;
  radius: number;
  limit: number;
  types: AgentEntityType[];
  totalWithinRadius: number;
  returned: number;
  truncated: boolean;
  entities: AgentEntitySnapshot[];
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
