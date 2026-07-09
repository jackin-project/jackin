# jackin-isolation

Mount isolation subsystem. Materializes per-workspace isolated git worktree mounts from the host repository into a container, inspects git state, and cleans up on teardown — the mechanism behind per-mount isolation.

## What this crate owns

- Mount materialization (`materialize`) from host repo into isolated worktrees, with branch/state tracking (`branch`, `state`).
- Git inspection (`git_inspect`) for dirty/worktree state.
- Finalization and cleanup (`finalize`, `cleanup`) on session teardown.

## Architecture tier and allowed dependencies

**Application/integration crate** sitting between orchestration and the Docker/git substrate. Allowed production dependencies (inward only): `jackin-core`, `jackin-config`, `jackin-diagnostics`, `jackin-protocol`, `jackin-docker`, `jackin-runtime`. Must NOT depend on `jackin-launch-tui` or `jackin-tui` (presentation).

## Structure

- `src/materialize.rs` / `src/materialize/` — mount materialization
- `src/branch.rs`, `src/state.rs` — branch + isolation-state tracking
- `src/git_inspect.rs` / `src/git_inspect/` — git/worktree inspection
- `src/finalize.rs`, `src/cleanup.rs` — finalize + teardown

## Public API

Materialize/finalize/cleanup entry points consumed by `jackin-runtime`. Parallel materialization must partition by dependency/order group and host repo to avoid `.git` lock contention (tracked as a performance item in the code-health roadmap).

## How to verify

```sh
cargo nextest run -p jackin-isolation
cargo clippy -p jackin-isolation --all-targets -- -D warnings
```

See [../AGENTS.md](../AGENTS.md) for workspace-wide Rust rules and [../../AGENTS.md](../../AGENTS.md) for repo rules.
