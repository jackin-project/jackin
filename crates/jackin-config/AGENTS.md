# AGENTS.md — jackin-config

Configuration schema, validation, migration, and workspace resolution for jackin❯.

## Rules (this crate)

- Schema changes are versioned: a `config.toml`/per-workspace schema change ships the matching `migrations`/`versions` entry in the same PR (5-artifact rule, `PRERELEASE.md`).
- Resolution and validation are pure transforms; persistence is the only I/O and stays narrow.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
