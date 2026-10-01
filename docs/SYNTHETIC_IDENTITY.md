# Synthetic Development Identity

## Purpose

FiveM/FxDK GameRuntime sessions may not expose the same Rockstar-backed identifiers that a normal FiveM client exposes. Development servers often still expect `license` or `license2`, which otherwise forces every test server to implement an FXDK-specific fallback.

FXDK Agent should provide a development-only identity layer so ordinary server code can keep using its normal identifier path.

## Target behavior

For a local FXDK Agent session, a normal resource should be able to read identifiers and observe values such as:

```text
license:deadbeef00000001aabbccddeeff001122334455
license2:deadbeef00000001aabbccddeeff001122334455
```

The payload remains 40 hexadecimal characters for compatibility with common validation logic, while the `deadbeef` prefix makes the value visibly development-only in logs and databases.

`deadbeef` is an FXDK Agent project convention. It is not an official Cfx.re or Rockstar identifier namespace.

## Determinism

Synthetic identities should be stable across restarts of the same local development installation and distinct across client slots.

Proposed conceptual layout:

```text
deadbeef + 8-hex client slot + 24 hex derived from local dev seed
```

Examples:

```text
slot 1: deadbeef00000001aabbccddeeff001122334455
slot 2: deadbeef00000002aabbccddeeff001122334455
```

The production implementation should derive the tail from a random local development seed generated once and stored in user configuration. It should not fingerprint hardware just to produce a stable ID.

A versioned derivation is preferred, for example conceptually:

```text
sha256("fxdk-agent:dev-license:v1" + localSeed + clientSlot)
```

Only the required hexadecimal portion would be embedded after the visible DEV prefix and slot.

## Compatibility goal

The preferred implementation makes the synthetic identifiers visible through the same server-side identifier surface used by ordinary resources, including Lua, JavaScript/TypeScript, and C# consumers.

The target is:

```text
FXDK Agent
    -> synthetic connection identity
    -> FXServer
    -> normal identifier APIs
    -> arbitrary server resources
```

A target server should not need application-specific code such as `if fxdk then useFallbackLicense()`.

If the Cfx/FxDK runtime does not expose a supported native injection point, the project may temporarily use an automatically mounted compatibility resource. Such a fallback must be documented as compatibility mode and must not be presented as equivalent to a true connection-level identifier.

## `license` and `license2`

For local development compatibility, the initial design may expose the same synthetic payload for both `license` and `license2`. This avoids making server projects care which identifier a GameRuntime session happened to omit.

The exact behavior remains subject to validation against supported Cfx/FxDK runtime contracts.

## Guardrails

Synthetic identity must be fail-closed and development-only.

Required guards:

- explicit opt-in in FXDK Agent configuration;
- local/loopback session only;
- FXDK/development mode only;
- never claim to be a real Rockstar, Cfx.re, Steam, Discord, or other third-party identity;
- never copy a real user's license value into generated identities;
- never enable synthetic identity silently on a public/production server.

## Multi-client testing

Each client slot should receive a stable, distinct identity. This allows databases, permissions systems, character systems, and session layers to behave as if separate development players connected.

```text
local seed
   |
   +-- slot 1 -> DEV license A
   +-- slot 2 -> DEV license B
   +-- slot 3 -> DEV license C
   +-- slot N -> DEV license N
```

## Database visibility

Because the value is intentionally recognizable, accidental persistence into a development database remains obvious. Consumers should still treat it as an opaque identifier; the `deadbeef` prefix is for operator visibility, not for application branching.

## Open implementation question

Before implementation, the project must determine the lowest supported Cfx/FxDK layer capable of exposing the identifier consistently to Lua, JavaScript/TypeScript, and C#. Binary patching of FiveM/FXServer is not the preferred starting point.
