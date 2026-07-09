# AGENTS.md — jackin-isolation

Mount isolation subsystem — materialize isolated worktree mounts, inspect git, clean up.

## Rules (this crate)

- Avoid `.git` lock contention: parallel materialization partitions by dependency/order group and host repository; do not materialize the same host repo concurrently without partitioning.
- Teardown is deterministic: `finalize`/`cleanup` must be safe on a partially-materialized state; never leak a worktree/branch on a failure path.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
