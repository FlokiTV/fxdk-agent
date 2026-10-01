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
- **Svelte 5 + TypeScript** — desktop UI and reactive application state.
- **Rspack 2** — frontend bundler and development server.
- **TypeScript** — protocol schemas, SDK, documentation tooling, automation, and end-to-end tests.
- **Tauri 2** — desktop shell for the MVP; the Windows scaffold has been validated locally.

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

The repository is currently **WIP** and the MVP implementation is underway.

Foundation already in place:

- Cargo workspace with a native Rust host crate;
- Tauri 2 desktop shell with Svelte 5 + TypeScript bundled by Rspack 2;
- shared TypeScript protocol package;
- loopback Control API on `127.0.0.1:35418`;
- runtime discovery through `/agent.md` and `/openapi.json`;
- versioned local configuration with `GET/PATCH /v1/config`;
- runtime path validation before config persistence;
- Svelte environment configuration view;
- root-level typecheck/check/build commands;
- local host + frontend smoke validated on Windows.

The next milestone is process supervision and FXServer lifecycle: owned process trees, readiness detection, crash state, deterministic stop, and orphan cleanup.

## Development

Current development prerequisites:

- Windows;
- Rust stable with the MSVC target/toolchain;
- Bun 1.4 or newer;
- Tauri Windows prerequisites, including the Microsoft C++ build tools and WebView2 runtime.

Install dependencies:

```bash
bun install
```

Validate the workspace:

```bash
bun run check
```

Build the TypeScript frontend and Rust workspace:

```bash
bun run build
```

Run the desktop shell in development mode:

```bash
bun run dev:desktop
```

### Local configuration

The host creates a versioned local config file in the operating system's local application-data directory. On Windows this resolves under `%LOCALAPPDATA%\FXDK Agent\config.json`.

The public configuration contract contains:

- server project directory;
- FXServer executable path;
- FiveM executable path;
- synthetic development identity toggle.

The same contract is available through:

```http
GET /v1/config
PATCH /v1/config
```

Populated paths are validated before they are persisted. Invalid patches return HTTP `422` with field-specific validation issues.

## License

FXDK Agent is licensed under the **Apache License 2.0**. Commercial use, modification, and redistribution are permitted subject to the license terms.

Distributions must preserve the applicable license and attribution notices. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).

## Non-affiliation

FXDK Agent is an independent community project. It is not affiliated with or endorsed by Cfx.re, Rockstar Games, or Take-Two Interactive.
