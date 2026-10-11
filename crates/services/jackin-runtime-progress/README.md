# jackin-runtime-progress

Host wiring for launch
progress surfaces.

## What this crate owns

- Singletons (`progress`):
  `host_terminal`,
  `launch_output`.
- Dialogs (`progress`):
  standalone select, error
  popup, exit and launch
  dialogs.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`progress.rs`](src/progress.rs) | host wiring | — |

## Public API

`progress::host_terminal`,
`progress::launch_output`,
`progress::LaunchProgress`,
`progress::standalone_select_with_context`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-progress
cargo clippy -p jackin-runtime-progress --all-targets -- -D warnings
```
