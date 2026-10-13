# jackin-usage-provider-codex

`Codex` / `OpenAI` usage snapshot collection: OAuth credentials, RPC
transport, and profile snapshots. Consumed by `jackin-usage` (host
discovery, credential snapshots).

## What this crate owns

- OAuth credentials (`credentials`): credential loading + account labels.
- Endpoints (`endpoints`): base/usage/reset endpoint resolution.
- OAuth fetch (`oauth`): usage fetch, token refresh.
- RPC transport (`rpc`, `rpc_types`): app-server RPC + response types.
- Usage types (`types`): usage responses, limits, windows.
- Reset windows (`windows`): reset-credit windows.
- Snapshots (`snapshot`): identity + profile snapshots.
- Main view (`views`): `codex_snapshot` entry point.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`credentials.rs`](src/credentials.rs) | OAuth credential loading | — |
| [`endpoints.rs`](src/endpoints.rs) | endpoint resolution | — |
| [`oauth.rs`](src/oauth.rs) | OAuth usage fetch | — |
| [`rpc.rs`](src/rpc.rs) · [`rpc_types.rs`](src/rpc_types.rs) | RPC transport + types | — |
| [`types.rs`](src/types.rs) | usage response types | — |
| [`windows.rs`](src/windows.rs) | reset-credit windows | — |
| [`snapshot.rs`](src/snapshot.rs) | identity + profile snapshots | — |
| [`views.rs`](src/views.rs) | main snapshot view | — |

## Public API

`codex_snapshot`, `codex_profile_snapshot`, OAuth/RPC helpers, and usage
types consumed by `jackin-usage`.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-codex
cargo clippy -p jackin-usage-provider-codex --all-targets -- -D warnings
```
