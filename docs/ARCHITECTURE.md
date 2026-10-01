# Architecture

## Overview

FXDK Agent is split into a native host, TypeScript-facing tooling, and an optional in-game integration layer.

```text
+----------------------------------------------------------+
|                    FXDK Agent Desktop                    |
|                                                          |
|  +----------------------+    +------------------------+  |
|  | TypeScript UI        |    | TypeScript SDK         |  |
|  | Tauri/WebView        |    | schemas / clients      |  |
|  +----------+-----------+    +-----------+------------+  |
|             |                            |               |
|             +------------ HTTP ----------+               |
|                          |                               |
|                 +--------v---------+                     |
|                 | Rust Host        |                     |
|                 | Control Plane    |                     |
|                 +--------+---------+                     |
|                          |                               |
|          +---------------+----------------+              |
|          |               |                |              |
|          v               v                v              |
|      FXServer        FiveM/FxDK       Win32/Input        |
+----------+---------------+-------------------------------+
           |
           v
 optional injected resource
           |
           v
     In-game Agent API
```

## Rust components

### host

Bootstraps the executable, validates configuration, owns global state, starts the Control API, and coordinates the other services. UI-specific logic does not belong here.

### process-supervisor

Owns FXServer and FiveM/FxDK process trees, PIDs, readiness, exit codes, shutdown, restart, crash state, and orphan cleanup.

A core invariant is that stopping a managed session must not leave an orphan `FiveM.exe -fxdk`, GameRuntime, browser subprocess, or FXServer behind.

### runtime-overlay

Builds temporary runtime/server-data views without permanently modifying the target server project. It may mount optional development resources, generated configuration, and transient capabilities.

### windows-platform

Contains Windows-specific process/window discovery, Win32 integration, XInput, and low-level input helpers behind explicit interfaces.

### control-api

Loopback HTTP server for health, status, lifecycle, sanitized configuration, discovery, documentation, and future event streaming.

## TypeScript components

### apps/ui

Human-facing desktop UI. It must not directly own subprocess lifecycle; it calls the same service layer exposed through the Control API.

### packages/protocol

Canonical request/response types, state models, stable error codes, event schemas, and protocol versioning.

### packages/sdk

High-level client for agents, test runners, and external automation.

### e2e

Black-box acceptance tests that consume public surfaces instead of private host internals.

## Control boundaries

### Launcher Control API

Available as soon as the application is ready. It controls launcher state, managed processes, sessions, clients, configuration, health, and discovery.

### In-game Agent API

Available only after FXServer is online and the optional agent resource is ready. It handles game-level inspection and test actions such as players, resources, events, exports, natives, snapshots, and controlled test helpers.

The Control API reports the in-game Agent API state but does not absorb its responsibilities.

## State model

```text
launcher: starting | ready | stopping | error
server:   stopped | starting | online | stopping | crashed
client:   stopped | starting | connecting | active | stopping | crashed
agent:    disabled | starting | ready | error
```

## Local security model

- bind control services to loopback by default;
- use explicit capabilities/tokens for sensitive operations;
- keep arbitrary in-game code execution disabled by default;
- never expose secrets in status or logs;
- validate all target paths and executable paths;
- keep target server projects immutable unless the user explicitly requests a change;
- development identity injection must be gated to explicit local development mode.

## Distribution target

The end-user distribution should be a native Windows executable that does not require Python, Node.js, or Bun to be preinstalled.
