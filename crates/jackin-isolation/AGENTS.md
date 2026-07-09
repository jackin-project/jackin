# AGENTS.md — jackin-isolation

Mount isolation subsystem — materialize isolated worktree mounts, inspect git, clean up.

## Hard rules (this crate)

- **Tier & dependencies:** application/integration. Allowed production deps (inward only): `jackin-core`, `jackin-config`, `jackin-diagnostics`, `jackin-protocol`, `jackin-docker`, `jackin-runtime`. Must NOT depend on `jackin-launch-tui` or `jackin-tui`.
- **Keep `README.md` current:** update it when structure, public API, materialization, or cleanup change (see `crates/AGENTS.md`).
- **Avoid `.git` lock contention.** Parallel materialization partitions by dependency/order group and host repository; do not materialize the same host repo concurrently without partitioning.
- **Teardown is deterministic.** `finalize`/`cleanup` must be safe to call on a partially-materialized state; never leak a worktree/branch on a failure path.

## What lives here vs elsewhere

- This crate owns: worktree materialization, branch/isolation-state, git inspection, finalize/cleanup.
- Docker run lives in `jackin-docker`/`jackin-runtime`. Instance identity lives in `jackin-instance`. Per-mount isolation *behavior* documented for operators in the workspaces guide.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
