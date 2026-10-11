# jackin-runtime-backend-selection

Persisted container
backend selection:
Docker vs Apple
Container for a
runtime instance.

## What this crate owns

- Selection
  (`backend_selection`):
  `backend_for_manifest` /
  `backend_for_state` —
  resolve the backend
  recorded in the
  instance manifest,
  defaulting legacy
  manifests to Docker.
  The runtime hub keeps
  the lifecycle dispatch
  (S7 split 98).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`backend_selection.rs`](src/backend_selection.rs) | selection | hub `backend` suite (`backend/tests.rs`) |

## Public API

`backend_selection::InstanceBackend`,
`backend_selection::backend_for_manifest`,
`backend_selection::backend_for_state`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-backend-selection
cargo clippy -p jackin-runtime-backend-selection --all-targets -- -D warnings
```
