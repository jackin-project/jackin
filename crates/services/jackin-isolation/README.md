# jackin-isolation

Mount isolation subsystem. Materializes per-workspace isolated git worktree mounts from the host repository into a container, inspects git state, and cleans up on teardown — the mechanism behind per-mount isolation.

## What this crate owns

- Mount materialization (`materialize`) from host repo into isolated worktrees, with branch/state tracking (`branch`, `state`).
- Git inspection (`git_inspect`) for dirty/worktree state.
- Finalization and cleanup (`finalize`, `cleanup`) on session teardown.

## Architecture tier and allowed dependencies

**Application/integration crate** sitting between orchestration and the Docker/git substrate. Allowed production dependencies (inward only): `jackin-core`, `jackin-config`, `jackin-instance`, `jackin-diagnostics`, `jackin-protocol`, `jackin-docker`, `jackin-process`, `jackin-telemetry`. Must NOT depend on `jackin-launch`, `jackin-console`, or TermRock presentation crates.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`materialize.rs`](src/materialize.rs) · [`materialize/`](src/materialize) | mount materialization | — |
| [`branch.rs`](src/branch.rs) · [`branch/`](src/branch) | branch tracking | — |
| [`state.rs`](src/state.rs) · [`state/`](src/state) | isolation-state tracking | — |
| [`git_inspect.rs`](src/git_inspect.rs) · [`git_inspect/`](src/git_inspect) | git/worktree inspection | — |
| [`finalize.rs`](src/finalize.rs) · [`finalize/`](src/finalize) | finalize | — |
| [`cleanup.rs`](src/cleanup.rs) · [`cleanup/`](src/cleanup) | teardown/cleanup | — |

## Public API

Materialize/finalize/cleanup entry points consumed by `jackin-runtime`. Parallel materialization must partition by dependency/order group and host repo to avoid `.git` lock contention (tracked as a performance item in the code-health roadmap).

Worktree add/adopt/prune and cleanup share a persistent OS lock in the repository's common Git directory. This serializes Jackin callers that use the lock. Concurrent out-of-band Git worktree mutations are unsupported: Git does not honor the lock, and an external `git worktree add --force` can still add a scratch-branch checkout between cleanup's final inventory and ref deletion. Jackin retains its cleanup journal and fails closed when its checkout inventory or exact registration identity no longer matches.

## How to verify

```sh
cargo nextest run -p jackin-isolation
cargo clippy -p jackin-isolation --all-targets -- -D warnings
```
