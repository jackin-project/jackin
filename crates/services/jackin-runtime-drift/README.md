# jackin-runtime-drift

Workspace isolation
drift detection across
preserved state.

## What this crate owns

- Detect (`drift`):
  `detect_workspace_edit_drift`
  — classify drifted
  records into running
  containers vs stopped
  records.
- Shape (`drift`):
  re-exports
  `DriftDetection`
  from `jackin-core`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`drift.rs`](src/drift.rs) | classifier | `drift/tests/` |
| [`drift/`](src/drift/) | test suites | `drift/tests/` |

## Public API

`drift::detect_workspace_edit_drift`,
`drift::DriftDetection`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-drift
cargo clippy -p jackin-runtime-drift --all-targets -- -D warnings
```
