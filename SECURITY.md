# Security Policy

FXDK Agent controls local processes and may eventually expose powerful in-game development capabilities. Security boundaries are part of the product contract.

## Supported versions

The project is pre-release WIP. Security fixes currently apply to the latest development branch only.

## Reporting a vulnerability

Do not publish credentials, tokens, private server data, or exploit details in a public issue.

When the repository is hosted on GitHub, prefer GitHub Private Vulnerability Reporting / Security Advisories when enabled. If that channel is unavailable, contact the repository maintainers privately before disclosing details publicly.

## Security expectations

- Control APIs bind to loopback by default.
- Secrets must not appear in logs, status payloads, crash reports, or committed configuration.
- Arbitrary in-game execution must be opt-in and capability-gated.
- Target project files are not modified permanently without explicit user intent.
- Managed process trees must be cleaned up deterministically.
- Synthetic identities are local-development identities only and must never impersonate real third-party identities.

## Out of scope

This project is not intended to bypass Cfx.re, Rockstar Games, platform authentication, anti-cheat systems, licensing, or access controls. Development identity support exists only to make local FXDK testing deterministic.
