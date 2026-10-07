# jackin-runtime-attach-transport

Host attach
transport selection.

## What this crate owns

- Plan
  (`transport`):
  `HostAttachTransportPlan`,
  `select_host_attach_transport`,
  `MAX_UNIX_SOCKET_PATH_LEN` —
  direct-socket vs
  attach-proxy choice.
- Exec
  (`transport`):
  `attach_proxy_exec_args`,
  `JACKIN_CAPSULE_PATH`,
  `ATTACH_PROXY_SUBCOMMAND` —
  proxy invocation.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`transport.rs`](src/transport.rs) | plan + exec | hub `attach` suite |

## Public API

`transport::HostAttachTransportPlan`,
`transport::select_host_attach_transport`,
`transport::MAX_UNIX_SOCKET_PATH_LEN`,
`transport::attach_proxy_exec_args`,
`transport::JACKIN_CAPSULE_PATH`,
`transport::ATTACH_PROXY_SUBCOMMAND`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-attach-transport
cargo clippy -p jackin-runtime-attach-transport --all-targets -- -D warnings
```
