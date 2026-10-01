# Contributing

FXDK Agent is currently a work-in-progress project. Contributions are welcome, but public contracts may still evolve quickly.

## Before opening a pull request

1. Check the roadmap and existing issues.
2. Keep changes focused on one concern.
3. Document new public behavior or protocol changes.
4. Add or update tests for lifecycle, cleanup, and error cases.
5. Never commit credentials, machine-specific paths, private server data, game assets, or proprietary binaries.

## Architecture rules

- process lifecycle belongs in the Rust host, not in the UI;
- the TypeScript UI should consume stable host services/API contracts;
- the CLI should reuse the Control API rather than implement lifecycle separately;
- target server projects should remain immutable by default;
- in-game capabilities must remain separate from launcher/process capabilities;
- Windows-specific code should stay behind a platform boundary;
- synthetic identity must remain explicitly development-only.

## Development workflow

The exact build commands will be added with the first Rust/TypeScript scaffold. Until then, documentation changes should keep Markdown links valid and terminology consistent.

## Pull requests

A useful PR description should include:

- what changed;
- why it changed;
- user-visible/API impact;
- validation performed;
- known limitations or follow-up work.

## Licensing of contributions

Unless explicitly stated otherwise, contributions intentionally submitted for inclusion in FXDK Agent are provided under the Apache License 2.0, consistent with Section 5 of the project license.

Do not submit code or assets that you do not have the right to license under those terms.

## Proprietary content

Do not submit Rockstar Games, FiveM, Cfx.re, server-owner, or other third-party proprietary assets/binaries unless their redistribution is explicitly permitted.
