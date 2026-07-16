# jackin-isolation

Mount isolation subsystem. Materializes per-workspace isolated git worktree mounts from the host repository into a container, inspects git state, and cleans up on teardown — the mechanism behind per-mount isolation.

## What this crate owns

- Mount materialization (`materialize`) from host repo into isolated worktrees, with branch/state tracking (`branch`, `state`).
- Git inspection (`git_inspect`) for dirty/worktree state.
- Finalization and cleanup (`finalize`, `cleanup`) on session teardown.

## Architecture tier and allowed dependencies

**Application/integration crate** sitting between orchestration and the Docker/git substrate. Allowed production dependencies (inward only): `jackin-core`, `jackin-config`, `jackin-diagnostics`, `jackin-protocol`, `jackin-docker`, `jackin-runtime`. Must NOT depend on `jackin-launch-tui`, `jackin-console`, or TermRock presentation crates.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`materialize.rs`](src/materialize.rs) · [`materialize/`](src/materialize) | mount materialization | [`tests.rs`](src/materialize/tests.rs) |
| [`branch.rs`](src/branch.rs) · [`branch/`](src/branch) | branch tracking | [`tests.rs`](src/branch/tests.rs) |
| [`state.rs`](src/state.rs) · [`state/`](src/state) | isolation-state tracking | [`tests.rs`](src/state/tests.rs) |
| [`git_inspect.rs`](src/git_inspect.rs) · [`git_inspect/`](src/git_inspect) | git/worktree inspection | [`tests.rs`](src/git_inspect/tests.rs) |
| [`finalize.rs`](src/finalize.rs) · [`finalize/`](src/finalize) | finalize | [`tests.rs`](src/finalize/tests.rs) |
| [`cleanup.rs`](src/cleanup.rs) · [`cleanup/`](src/cleanup) | teardown/cleanup | [`tests.rs`](src/cleanup/tests.rs) |

## Public API

Materialize/finalize/cleanup entry points consumed by `jackin-runtime`. Parallel materialization must partition by dependency/order group and host repo to avoid `.git` lock contention (tracked as a performance item in the code-health roadmap).

## How to verify

```sh
cargo nextest run -p jackin-isolation
cargo clippy -p jackin-isolation --all-targets -- -D warnings
```

