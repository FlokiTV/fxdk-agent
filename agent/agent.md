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

## Operating rules

- Treat the Control API as the lifecycle authority; do not duplicate process-management logic externally.
- Do not modify the target server project unless an explicit API operation says it will.
- Wait for state transitions reported by `/v1/status` instead of assuming a process is ready immediately after start.
- Stop managed sessions through the Control API so owned process trees can be cleaned up deterministically.
- Never treat synthetic development identities as production Rockstar/Cfx identities.
- The Control API is intended for local development and binds to loopback by default.

## In-game Agent API

The in-game Agent API is a separate optional capability. It is only usable after FXServer is running and the runtime reports the agent state as ready.

Do not assume the in-game Agent API is available merely because the launcher Control API is healthy.
