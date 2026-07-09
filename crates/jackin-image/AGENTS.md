# AGENTS.md — jackin-image

Image generation and binary-artifact management for jackin❯.

## Rules (this crate)

- Derived vs construct boundary: this crate generates *derived* images on top of the operator-built `construct` base (`docker/construct/`); do not blur the two.
- Binary acquisition is cached and version-checked: agent/capsule binary fetches go through the acquisition + version-check helpers, never an inlined one-off download.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
