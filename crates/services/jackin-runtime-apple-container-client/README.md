# jackin-runtime-apple-container-client

Apple Container
backend client:
lifecycle over the
`container` CLI.

## What this crate owns

- Client
  (`apple_container_client`):
  `AppleContainerClient`,
  `AppleContainerApi`,
  `AppleContainerSpec`,
  `AppleContainerMount`,
  `AppleContainerInfo` —
  run/stop/remove/
  inspect/list via
  the shared process
  transport.
- Parsing
  (`apple_container_client`):
  `container ps
  --format json`
  array + NDJSON
  shapes, capitalized
  keys, `unknown`
  default.
- Test double
  (`apple_container_client`,
  `test-support`):
  `FakeAppleContainerClient`
  in-memory lifecycle.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`apple_container_client.rs`](src/apple_container_client.rs) | client | `apple_container_client/tests/` |
| [`apple_container_client/`](src/apple_container_client/) | test suites | `apple_container_client/tests/` |

## Public API

`apple_container_client::AppleContainerClient`,
`apple_container_client::AppleContainerApi`,
`apple_container_client::AppleContainerSpec`,
`apple_container_client::BACKEND_NAME`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-client
cargo clippy -p jackin-runtime-apple-container-client --all-targets -- -D warnings
```
