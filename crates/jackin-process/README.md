# jackin-process

Shared subprocess transport for jackin❯: capture, timeout, retry, and exit status.

## What this crate owns

- `ExecRequest` / `ExecResult` and the async + sync run helpers used by xtask, capsule probes, and runtime shell execution.
- Independent capture byte limits (16 MiB default per stream), elapsed timeout, retry, and kill/reap ownership for run helpers. Unix runs own an isolated process group through pipe completion.
- Spawn helpers return registered `SyncChild` / `AsyncChild` owners with caller-owned streams. Dropping an unfinished owner terminates its child and transfers eventual reap ownership to a dedicated waiter. `SyncChild::detach` and `AsyncChild::detach` keep the child alive on drop while transferring eventual reap ownership to that waiter. Explicit capture limits are rejected because callers own reads.
- `spawn_group_async` / `spawn_group_sync` return `GroupChild` / `GroupSyncChild`. Drain captured streams before consuming `finish`; async completion also supports `finish_with_timeout`. Cancellation and `kill_and_reap` terminate the private process group.
- `spawn_foreground_async` gives an inherited controlling terminal to the private child group before exec and restores the invoking group after completion, timeout, or cancellation.
- Shared child reservations serialize spawn registration with PID 1 orphan reaping, preserving owners' exit statuses and group identifiers.
- Callers own redaction, protected-value classification, environment policy, and telemetry.

## Architecture tier

**T0 foundational.** Uses `anyhow`, `tokio`, `nix` on Unix, and the foundational `jackin-process-directory` descriptor helper. Owns no policy or telemetry.

## Structure

| Module | Owns |
|---|---|
| [`lib.rs`](src/lib.rs) | request/result types, async core, sync facade |
| [`child_ownership.rs`](src/child_ownership.rs) | spawn/reaper coordination and counted PID reservations |
| [`spawned_child.rs`](src/spawned_child.rs) | registered sync, async, and process-group owners |

## How to verify

```sh
cargo nextest run -p jackin-process
cargo clippy -p jackin-process --all-targets -- -D warnings
```
