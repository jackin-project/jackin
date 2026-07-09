# AGENTS.md — jackin-config

Configuration schema, validation, migration, and workspace resolution for jackin❯.

## Hard rules (this crate)

- **Tier & dependencies:** L0 domain (schema). Allowed workspace deps: `jackin-core`. No presentation, infrastructure adapters, or observability — they live above.
- **Keep `README.md` current:** update it when structure, public API, module layout, or responsibilities change (see `crates/AGENTS.md`).
- **Schema changes are versioned.** `config.toml` and per-workspace config are versioned artifacts; a schema change ships the matching `migrations`/`versions` entry in the same PR. See `PRERELEASE.md` for the 5-artifact schema-bump rule.
- **No runtime side effects.** Resolution and validation are pure transforms; persistence is the only I/O and stays narrow.

## What lives here vs elsewhere

- This crate owns: config/workspace schema, resolution, validation, migration, persistence.
- Role manifests live in `jackin-manifest`. Runtime *use* of resolved config lives in `jackin-runtime`. Editor presentation of config lives in `jackin-console`/`jackin-tui`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
