# AGENTS.md — jackin-core

Universal vocabulary types shared across every jackin❯ crate. The leaf at the bottom of the workspace graph.

## Hard rules (this crate)

- **Tier & dependencies:** L0 leaf/domain. Allowed workspace deps: **none**. No `tokio`, no subprocess, no filesystem, no presentation. Do not add any jackin❯ dependency — this is the floor.
- **Keep `README.md` current:** update it when structure, public API, module layout, or responsibilities change (see `crates/AGENTS.md`).
- **No behavior here.** Types, traits, constants, and pure helpers only. Anything that performs I/O, spawns a process, or holds a runtime belongs in a higher crate.
- **Cheap to compile.** Every crate depends on this one; keep it small and dependency-free so it never becomes a build bottleneck.

## What lives here vs elsewhere

- This crate owns: shared domain types, port traits, constants, paths, and reusable pure helpers.
- Concrete adapters (Docker, runners, host OS) live in `jackin-docker`, `jackin-host`, `jackin-runtime`. Presentation of tokens lives in `jackin-tui`. Observability implementation lives in `jackin-diagnostics`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
