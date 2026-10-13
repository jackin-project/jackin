# jackin-runtime-coordination

Lock files and state
directories for runtime
coordination.

## What this crate owns

- Roots (`coordination`):
  `root`, `universe_dir`,
  `ensure_prunable` (+ async).
- Locks (`coordination`):
  `open_lock`,
  `open_in_namespace`,
  `open_directory_in_namespace`,
  `open_state_in_namespace`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`coordination.rs`](src/coordination.rs) | roots + locks | `coordination/tests/` |
| [`coordination/`](src/coordination/) | test suites | `coordination/tests/` |

## Public API

`coordination::open_lock`,
`coordination::universe_dir`,
`coordination::root`,
`coordination::ensure_prunable`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-coordination
cargo clippy -p jackin-runtime-coordination --all-targets -- -D warnings
```
