# Synthetic Development Identity

## Status

The MVP implementation now provides a deterministic, development-only identity owned by FXDK Agent.

It does **not** currently inject `license` or `license2` into FXServer's native player identifier list. Runtime validation confirmed that FxDK connections still expose only the native identifiers provided by Cfx itself.

This distinction is intentional and documented explicitly.

## Current contract

When synthetic identity is enabled, FXDK Agent derives one identity per client slot.

The payload is exactly 40 lowercase hexadecimal characters:

```text
deadbeef + 8-hex client slot + 24 derived hex characters
```

Example shape:

```text
deadbeef00000001aabbccddeeff001122334455
```

The same payload is exposed conceptually as both:

```text
license:<payload>
license2:<payload>
```

The `deadbeef` prefix is an FXDK Agent convention. It is not an official Rockstar or Cfx.re namespace.

## Derivation

The implementation uses a versioned namespace and a private local seed.

Conceptually:

```text
digest = SHA-256(
  "fxdk-agent/dev-identity/v1"
  + localSeed
  + clientSlot
)

payload =
  "deadbeef"
  + slot as 8 lowercase hex characters
  + first 24 hex characters derived from digest
```

Properties:

- deterministic for the same local seed and client slot;
- different across client slots;
- exactly 40 lowercase hexadecimal characters;
- visibly development-only;
- independent of process IDs, ports, usernames, or hardware fingerprints.

## Local seed

The seed is generated from the operating system random source and persisted separately from the public app configuration.

On Windows the default store is:

```text
%LOCALAPPDATA%\FXDK Agent\identity.json
```

The seed is not exposed through the Control API or frontend configuration contract.

Resetting the identity store rotates the seed and therefore changes all derived local development identities. Any local database rows keyed by the previous synthetic identifiers will no longer refer to the new identities after a reset.

## FXServer transport

When `syntheticIdentity.enabled` is `true`, `POST /v1/server/start` resolves the identity for client slot 1 and passes it to the managed FXServer launch.

The following development-only convars are applied both before and after `server.cfg`:

```text
fxdk_agent_dev_identity = 1
fxdk_agent_dev_slot     = 1
fxdk_agent_dev_license  = <40-hex payload>
fxdk_agent_dev_license2 = <40-hex payload>
```

Applying them again after `server.cfg` prevents an unrelated server config value from silently replacing the session identity.

The FXServer controller also enforces the existing local development launch contract:

```text
sv_lan = 1
sv_fxdkMode = 1
```

The configured server address is validated as loopback-only before it can be persisted.

## Runtime validation

The MVP was validated with a temporary FXServer project containing independent Lua and JavaScript observer resources.

With synthetic identity enabled, both runtimes observed the same stable convars:

```text
enabled=1
slot=1
license=<deadbeef... payload>
license2=<same deadbeef... payload>
```

A real FxDK client then connected and reached the active connection state.

### Lua native identifier result

The Lua resource observed:

```text
GetPlayerIdentifiers(source)
=> ["ip:127.0.0.1"]

GetPlayerIdentifierByType(source, "license")
=> nil

GetPlayerIdentifierByType(source, "license2")
=> nil
```

### JavaScript native identifier result

The JavaScript resource enumerated identifiers through:

```text
GetNumPlayerIdentifiers(source)
GetPlayerIdentifier(source, index)
```

and observed:

```text
["ip:127.0.0.1"]
```

No native `license:` or `license2:` entry was present.

The client still reached:

```text
gameProcessState = 2
connectionState = 8
client state = active
```

so the missing identifiers are an identity-layer limitation, not a failed client connection.

## Native injection limitation

FXServer's scripting runtimes consume the identifier list supplied by the server connection/identity layer. The current FxDK connection does not populate Rockstar-backed `license` / `license2` identifiers, and the normal Lua or JavaScript scripting surface does not provide a supported operation for inserting arbitrary entries into that native list.

Therefore the current MVP does **not** claim that the development convars are native player identifiers.

In particular, this project does not describe any of the following as equivalent to native injection:

- monkey-patching only Lua helpers;
- adding a JavaScript wrapper around identifier reads;
- asking each target base to implement an FXDK-specific fallback;
- patching the FiveM or FXServer binary.

## Compatibility policy

The long-term target remains zero target-base adaptations:

```text
FXDK Agent
    -> connection-level synthetic identity
    -> FXServer native identifier list
    -> ordinary Lua / JS / C# identifier APIs
```

Reaching that target requires a supported connection/provider hook from Cfx/FxDK or a maintainable native extension point that can populate the server identity list before resources observe the player.

Until such a path is implemented and validated across Lua, JavaScript/TypeScript, and C#, FXDK Agent exposes its deterministic identity through the development convars above and documents native identifier injection as unavailable.

The project does not ship a runtime-specific monkey patch as if it solved the native identity problem.

## Existing project fallbacks

A target project may temporarily have its own DEV-only fallback while this native injection gap exists.

For example, a TypeScript core can explicitly read a configured local development identity when all of the following are true:

- development mode is active;
- LAN/FxDK mode is active;
- the connection is local;
- the fallback is explicitly enabled.

Such target-project logic is compatibility scaffolding, not the final FXDK Agent identity architecture, and should be removable once a true connection-level solution exists.

## Security boundaries

Synthetic identity is development-only.

Required boundaries:

- explicit opt-in through FXDK Agent configuration;
- loopback server address;
- `sv_lan=1`;
- `sv_fxdkMode=1`;
- no attempt to impersonate a real Rockstar, Cfx.re, Steam, Discord, or other identity;
- no copying of a real user's license value;
- no hardware fingerprinting solely to manufacture a stable identifier;
- no silent use against public or production servers.

## Multi-client direction

The derivation already supports stable identities for arbitrary positive client slots:

```text
local seed
   |
   +-- slot 1 -> DEV identity A
   +-- slot 2 -> DEV identity B
   +-- slot 3 -> DEV identity C
   +-- slot N -> DEV identity N
```

The MVP currently manages one client. Multi-client lifecycle support can reuse the same derivation without changing the identity format.
