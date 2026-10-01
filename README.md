# FXDK Agent

> Work in progress. The public API, configuration format, and internal architecture may change before the first stable release.

FXDK Agent is an open-source local development and QA harness for FiveM/FxDK environments. It is designed to be usable by humans, automation, CI, and coding agents without requiring manual interaction with the desktop UI.

## Goals

- start, stop, and supervise FXServer and FiveM/FxDK GameRuntime processes;
- expose a loopback-only Control API for automation;
- provide machine-readable discovery through `/openapi.json` and agent-oriented instructions through `/agent.md`;
- keep UI concerns separate from process and lifecycle logic;
- support deterministic development identities for local multi-client testing;
- optionally expose an in-game Agent API after the server is running;
- avoid requiring Python, Node.js, or Bun on end-user machines;
- remain safe for arbitrary server projects by avoiding permanent mutations to the target project.

## Planned stack

- **Rust** — native host, process supervision, Windows integration, filesystem/runtime overlays, input bridges, and the local Control API.
- **TypeScript** — desktop UI, protocol schemas, SDK, documentation tooling, automation, and end-to-end tests.
- **Tauri** — current candidate for the desktop shell. This is not final until the scaffold is validated.

## Architecture

FXDK Agent intentionally separates two control surfaces:

1. **Launcher Control API** — controls the local environment: processes, sessions, clients, configuration, and discovery.
2. **In-game Agent API** — becomes available only after FXServer is running and provides game-level inspection and test actions.

```text
Human / Agent / CI
       |
       v
Launcher Control API
       |
       +--> FXServer lifecycle
       +--> FiveM/FxDK lifecycle
       +--> status/config/process supervision
       |
       v
FiveM/FxDK session
       |
       v
Optional in-game Agent API
```

## Documentation

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — component boundaries and runtime model.
- [`docs/CONTROL_API.md`](docs/CONTROL_API.md) — initial HTTP control contract.
- [`docs/SYNTHETIC_IDENTITY.md`](docs/SYNTHETIC_IDENTITY.md) — deterministic development identity design.
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — public implementation roadmap.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — contribution workflow.
- [`SECURITY.md`](SECURITY.md) — security expectations and reporting guidance.

## Project status

The repository is currently **documentation-first WIP**. Runtime code has not been scaffolded yet. The first implementation milestone is the Rust host plus a minimal loopback Control API with:

- `GET /v1/health`
- `GET /v1/status`
- `GET /agent.md`
- `GET /openapi.json`

FiveM lifecycle support comes after that foundation is testable on its own.

## License

FXDK Agent is licensed under the **Apache License 2.0**. Commercial use, modification, and redistribution are permitted subject to the license terms.

Distributions must preserve the applicable license and attribution notices. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).

## Non-affiliation

FXDK Agent is an independent community project. It is not affiliated with or endorsed by Cfx.re, Rockstar Games, or Take-Two Interactive.
