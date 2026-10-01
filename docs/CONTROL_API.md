# Launcher Control API

## Purpose

The Control API is the primary automation surface of FXDK Agent. Once the application is open, an agent or CI worker should not need to click the desktop UI to start a server, launch a client, inspect state, or stop a session.

## Default bind

Initial proposal:

```text
127.0.0.1:35418
```

The final implementation may make the port configurable or dynamically negotiated.

## Discovery

### `GET /v1/health`

```json
{
  "ok": true,
  "version": "0.1.0"
}
```

### `GET /v1/status`

```json
{
  "launcher": { "state": "ready" },
  "server": {
    "state": "online",
    "address": "127.0.0.1:30120",
    "pid": 1234
  },
  "clients": [
    { "id": 1, "state": "active", "pid": 5678 }
  ],
  "agent": { "enabled": true, "state": "ready" }
}
```

### `GET /agent.md`

Returns concise instructions written for automation agents: what the tool controls, what endpoint to call first, how to start a session, how to wait for readiness, when the in-game Agent API becomes available, and how to cleanly stop the environment.

### `GET /openapi.json`

Returns the Control API OpenAPI document. An external agent should be able to discover the supported contract without reading repository source code.

## Lifecycle

### `POST /v1/server/start`

Starts only the managed FXServer.

### `POST /v1/server/stop`

Stops the managed FXServer and its owned process tree.

### `POST /v1/client/start`

```json
{ "client": 1 }
```

### `POST /v1/client/stop`

```json
{ "client": 1 }
```

### `POST /v1/session/start`

```json
{ "clients": 1 }
```

High-level orchestration should validate configuration, ensure FXServer readiness, launch the requested clients, track connection progress, and return a session identifier plus current state.

### `POST /v1/session/stop`

Stops all clients and server processes owned by the session.

## Configuration

- `GET /v1/config` returns sanitized configuration.
- `PATCH /v1/config` changes only explicitly supported fields.
- Secret values are never returned in plain text.

## Events

Polling `/v1/status` is sufficient for the first implementation. Server-Sent Events at `GET /v1/events` are a possible later addition. WebSocket support is not a V1 requirement.

## Error shape

```json
{
  "ok": false,
  "error": {
    "code": "SERVER_START_FAILED",
    "message": "FXServer exited before becoming ready",
    "detail": { "exitCode": 1 }
  }
}
```

Error codes should be stable enough for automation to branch on them.

## CLI

The future CLI should be a client of this same API rather than a separate lifecycle implementation:

```text
fxdk-agent status
fxdk-agent session start --clients 1
fxdk-agent session stop
fxdk-agent agent-guide
```
