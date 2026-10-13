# jackin-runtime-host-attach

Host-owned attach
client.

## What this crate owns

- Session
  (`host_attach`):
  `run_host_attach_session`,
  `host_attach_enabled`,
  `JACKIN_HOST_ATTACH_ENV` —
  operator terminal
  attach over socket
  or attach-proxy.
- Input
  (`host_attach/terminal_input`):
  `TerminalInput` —
  stdin mode + poll
  handling.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`host_attach.rs`](src/host_attach.rs) | session | [`tests.rs`](src/host_attach/tests.rs) (4 cases) |
| [`terminal_input.rs`](src/host_attach/terminal_input.rs) | input | [`tests.rs`](src/host_attach/terminal_input/tests.rs) |

## Public API

`host_attach::run_host_attach_session`,
`host_attach::host_attach_enabled`,
`host_attach::JACKIN_HOST_ATTACH_ENV`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-host-attach
cargo clippy -p jackin-runtime-host-attach --all-targets -- -D warnings
```
