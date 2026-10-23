# jackin-runtime-apple-container-running

Typed reconnect and post-attach observation.

`inspect_container` and `inspect_container_with` use the shared client listing and return `Result<Option<AppleContainerInfo>>` for an exact ID: errors mean listing failure, `None` means absence, and `Some` has `Running`, `Stopped`, `Unknown`, or `Stopping`. See the [inventory contract](../../../docs/content/research/platform/isolation/apple-container-backend.mdx).

Source: [`running.rs`](src/running.rs); callers: runtime and attach-outcome.

Public API: `running::inspect_container`, `running::inspect_container_with`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-running
cargo clippy -p jackin-runtime-apple-container-running --all-targets -- -D warnings
```
