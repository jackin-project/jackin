# jackin-runtime-apple-container-client

Apple Container lifecycle over the `container` CLI and shared process telemetry.

The client owns run/start/stop/remove/inspect/list. Listing validates the complete official JSON array and returns typed IDs/states; malformed UTF-8, shape, identity, or state is an error. Unknown metadata remains additive. See the [inventory contract](../../../docs/content/research/platform/isolation/apple-container-backend.mdx).

| Source | Tests |
|---|---|
| [`lib.rs`](src/lib.rs) | — |
| [`apple_container_client.rs`](src/apple_container_client.rs) | — |

Public API: `AppleContainerClient`, `AppleContainerApi`, `AppleContainerSpec`, `AppleContainerMount`, `AppleContainerInfo`, `AppleContainerStatus`, and `BACKEND_NAME` in `apple_container_client`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-client
cargo clippy -p jackin-runtime-apple-container-client --all-targets -- -D warnings
```
