# AGENTS.md — jackin-protocol

Shared wire-format contracts between the host CLI and `jackin-capsule`.

## Rules (this crate)

- A wire-format change is a host↔capsule contract change: align both binaries in the same PR.
- Data contracts only — no runtime behavior. Keep the crate dependency-light and serde-focused.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
