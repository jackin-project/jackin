# jackin-runtime-cleanup-socket-dir

Jackin runtime
host-side socket-directory
removal for cleanup.

## What this crate owns

- Removal
  (`remove_socket_dir`):
  `remove_socket_dir` —
  remove the per-container
  `sockets/<name>/`
  bind-mount directory
  behind the coordination
  gate, deleted through
  the contained safe-remove
  helper. Shared by the
  hub purge and eject
  flows (S7 split 105).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`socket_dir.rs`](src/socket_dir.rs) | removal | hub `cleanup` suite (`cleanup/tests/`) |

## Public API

`socket_dir::remove_socket_dir`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-cleanup-socket-dir
cargo clippy -p jackin-runtime-cleanup-socket-dir --all-targets -- -D warnings
```
