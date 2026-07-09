# AGENTS.md — jackin-xtask

The workspace's `cargo xtask` automation — CI lanes, lint/docs/schema gates, release + PR tooling.

## Rules (this crate)

- One entry point: project checks are discoverable via `cargo xtask <lane>`; a new check is a new lane here (and, where relevant, wired into `ci`). Prefer the narrowest correct lane an agent can run.
- `print_stdout` is scoped-allowed here (with a `reason`) because reports are the CLI's output — keep that carve-out to reporting only.
- Gates are ratchets: file-size/test-layout/agent-file gates are shrink-only with explicit, reasoned grandfathering; do not silently widen a budget or allowlist.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
