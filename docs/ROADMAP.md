# Roadmap

This roadmap describes intended milestones, not release commitments. FXDK Agent is currently WIP and the order may change as lower-level FiveM/FxDK constraints are validated.

## Milestone 0 — Repository and contracts

- publish architecture and security boundaries;
- define the Control API contract;
- define deterministic development identity behavior;
- establish contribution and testing conventions.

## Milestone 1 — Minimal Rust host

Create the initial Rust workspace with no FiveM dependency yet.

Required endpoints:

- `GET /v1/health`
- `GET /v1/status`
- `GET /agent.md`
- `GET /openapi.json`

The host should already have structured state, stable errors, configuration validation, and clean shutdown.

## Milestone 2 — FXServer lifecycle

- configure target server project and FXServer executable;
- create an isolated runtime view;
- launch FXServer with deterministic arguments;
- wait for server readiness;
- expose PID and lifecycle state;
- stop the full managed process tree;
- surface crashes with actionable diagnostics.

Acceptance:

```text
POST /v1/server/start
  -> server reaches online
  -> GET /v1/status reports online
  -> POST /v1/server/stop
  -> owned process tree is gone
```

## Milestone 3 — FiveM/FxDK client lifecycle

- discover FiveM;
- launch `-fxdk` clients;
- manage SDK URL and client numbering;
- detect GameRuntime connection state;
- guarantee orphan cleanup;
- support reconnect without rebuilding the entire environment.

## Milestone 4 — High-level sessions

Implement `POST /v1/session/start` and `POST /v1/session/stop` so automation does not need to know the internal startup order.

## Milestone 5 — Synthetic development identity

- deterministic development `license`/`license2` values;
- clearly recognizable DEV namespace;
- stable per local installation and client slot;
- transparent to normal Lua/JavaScript/C# identifier consumers when technically possible;
- strict local-development guards;
- no impersonation of real Rockstar/Cfx identities.

See [`SYNTHETIC_IDENTITY.md`](SYNTHETIC_IDENTITY.md).

## Milestone 6 — TypeScript UI and SDK

- validate Tauri as the desktop shell;
- status dashboard;
- session start/stop controls;
- visible `/agent.md` discovery URL;
- copyable agent bootstrap prompt;
- typed TypeScript SDK consuming the Control API.

## Milestone 7 — In-game Agent API

- optional runtime resource;
- game/resource/player inspection;
- controlled test actions;
- diagnostics and snapshots;
- explicit capability boundaries.

## Milestone 8 — Input and multi-client tooling

- XInput/input bridge;
- multiple isolated clients;
- per-client state and identity;
- deterministic cleanup;
- developer diagnostics.

## Milestone 9 — Distribution and release hardening

- native Windows distribution;
- portable/sanitized configuration;
- structured logs;
- crash diagnostics;
- repeatable build pipeline;
- license and third-party notices;
- end-to-end release acceptance.

## Promotion criteria for a first stable release

At minimum:

1. repeated FXServer start/stop passes;
2. repeated GameRuntime start/stop passes;
3. no managed orphan processes;
4. client reaches ACTIVE;
5. reconnect works;
6. synthetic development identity works across supported runtimes;
7. agent discovery is self-documenting;
8. a TypeScript FiveM server project passes smoke testing;
9. cleanup is deterministic;
10. a new automation agent can operate the tool using public documentation only.
