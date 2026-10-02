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

The repository is currently **WIP**, but the first Windows MVP is functional end to end.

Implemented and validated on Windows:

- native Rust host embedded directly in the Tauri desktop executable;
- Svelte 5 + TypeScript UI bundled by Rspack 2;
- loopback Control API on `127.0.0.1:35418`;
- FxDK Runtime Web endpoint on `127.0.0.1:35419`;
- machine-readable discovery through `/openapi.json` and `/agent.md`;
- versioned local configuration with path and loopback validation;
- Windows process-tree ownership and deterministic cleanup;
- managed FXServer lifecycle with readiness/crash state;
- managed FiveM/FxDK lifecycle with GameRuntime telemetry;
- high-level `session/start` and `session/stop` orchestration;
- deterministic development identities transported to the managed FXServer;
- session dashboard with environment configuration and Start/Stop controls;
- real repeated start -> ACTIVE -> stop smoke tests against a TypeScript FiveM server project;
- Windows release executable that runs without Python, Node.js, Bun, or Rust installed on the target machine.

The in-game Agent API, advanced multi-client support, input automation, and native Cfx identifier injection remain outside this MVP.

## Quickstart (WIP)

The current MVP is a portable Windows desktop executable. It embeds the Rust control plane, so there is no separate host process to start.

1. Run `fxdk-agent-desktop.exe`.
2. Configure the server project directory, FXServer executable, FiveM executable, loopback server address, and optional synthetic DEV identity.
3. Save the configuration.
4. Click **Start session**.

A successful managed start performs:

```text
desktop boot
-> Control API ready
-> Runtime Web ready
-> FXServer online
-> FiveM/FxDK launched
-> GameRuntime running
-> local client connection ACTIVE
```

Click **Stop** to tear down the managed FiveM/FxDK process tree and then FXServer.

The Control API remains available while the desktop is running at `http://127.0.0.1:35418`.

For agents and automation, start with:

```http
GET /agent.md
GET /v1/status
```

The high-level lifecycle endpoints are:

```http
POST /v1/session/start
Content-Type: application/json

{"clients":1}
```

and:

```http
POST /v1/session/stop
```

`session/start` returns only after the MVP client reaches ACTIVE, or returns an error after crash/timeout and rolls back managed processes.

### Synthetic DEV identity caveat

The MVP generates a deterministic 40-hex development identity per client slot and exposes it to the managed FXServer through FXDK Agent development convars.

Current FxDK connections still do not populate native `license:` / `license2:` values in `GetPlayerIdentifiers()`. See [`docs/SYNTHETIC_IDENTITY.md`](docs/SYNTHETIC_IDENTITY.md) for the validated Lua/JavaScript behavior and current limitation.

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

Build the portable Windows release executable:

```bash
bun run build:windows
```

The WIP release executable is produced at:

```text
target/release/fxdk-agent-desktop.exe
```

This command requires Rust/Cargo in `PATH` on the build machine. The generated executable does not require Bun, Node.js, Python, or Rust at runtime.

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
- loopback server address;
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
