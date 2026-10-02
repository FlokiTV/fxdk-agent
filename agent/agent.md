# FXDK Agent — Agent Guide

FXDK Agent is a local Windows control plane for FiveM/FxDK development and QA.

## Bootstrap

The default Control API is loopback-only:

```text
http://127.0.0.1:35418
```

Start by reading:

```http
GET /v1/status
```

Use the returned state as the source of truth for launcher, FXServer, clients, and the optional in-game Agent API.

## Discovery

```http
GET /v1/health
GET /v1/status
GET /agent.md
GET /openapi.json
```

Read `/openapi.json` before assuming an operation exists. The project is WIP and capabilities are added incrementally.

## Preferred managed lifecycle

For the MVP, prefer the high-level session endpoints over manually sequencing server and client operations.

Start one managed development session:

```http
POST /v1/session/start
Content-Type: application/json

{"clients":1}
```

A successful response is returned only after:

```text
FXServer online
-> FiveM/FxDK launched
-> GameRuntime running
-> client connection state ACTIVE
```

Check current state at any time:

```http
GET /v1/status
```

Stop the managed session:

```http
POST /v1/session/stop
```

The stop operation tears down the managed FiveM/FxDK process tree first and then FXServer.

Lower-level `/v1/server/*` and `/v1/client/*` endpoints remain available for diagnostics and focused testing. Do not mix those manual lifecycle calls into an active managed session.

## Configuration

Read or update the local development environment through:

```http
GET /v1/config
PATCH /v1/config
```

Configured runtime paths and the server address are validated before persistence. The managed server address is loopback-only.

## Operating rules

- Treat the Control API as the lifecycle authority; do not duplicate process-management logic externally.
- Do not modify the target server project unless an explicit API operation says it will.
- Wait for state transitions reported by `/v1/status` instead of assuming a process is ready immediately after start.
- Stop managed sessions through the Control API so owned process trees can be cleaned up deterministically.
- Never treat synthetic development identities as production Rockstar/Cfx identities.
- The Control API is intended for local development and binds to loopback by default.

## In-game Agent API

The in-game Agent API is a separate runtime capability transported through the loopback Control API. It becomes usable after the FxDK runtime registers itself.

Discover it first:

```http
GET /v1/agent/capabilities
```

A ready transport returns the managed client id plus the exact method names advertised by the runtime. Do not call methods that are absent from this list.

Invoke a method through:

```http
POST /v1/agent/invoke
Content-Type: application/json

{
  "clientId": 1,
  "method": "runtime.status",
  "params": {},
  "timeoutMs": 5000
}
```

The transport correlates every request with a unique request id and returns either `result` or a typed runtime `error`. Host-side transport failures use the normal Control API error envelope and stable codes such as `AGENT_RUNTIME_UNAVAILABLE`, `AGENT_METHOD_UNSUPPORTED`, and `AGENT_REQUEST_TIMEOUT`.

The runtime advertises the exact available method names. The core observation methods are:

- `runtime.ping` — verify request/response connectivity to the managed FxDK runtime.
- `runtime.status` — inspect launcher-side GameRuntime/connection state.
- `game.player` — bounded local-player snapshot including ped, coordinates, heading, health, armor, model, and current vehicle.
- `game.resources` — bounded resource name/state snapshot. `params.limit` is clamped to 1..512.
- `game.entities.nearby` — bounded nearby ped/vehicle/object snapshot. `params.radius` is clamped to 1..500 meters and `params.limit` to 1..256.

Gameplay observation calls require the managed client to be ACTIVE. Results include a runtime context with client id, server address, connection/game process state, and timestamp. The API intentionally does not expose arbitrary convar/environment dumps or secrets.

Endpoints under `/v1/agent/runtime/*` are reserved for the embedded FxDK runtime bridge. External agents should use `/v1/agent/capabilities` and `/v1/agent/invoke`.

The bridge is reset when a managed client/session starts, stops, crashes, or rolls back, so stale request ids must not be reused across runtime lifecycles.

Do not assume the in-game Agent API is available merely because the launcher Control API is healthy. Check `/v1/status` and `/v1/agent/capabilities`.
