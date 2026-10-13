# jackin-usage-host-runtime

Credential-free presentation for host clients that consume canonical usage
projections from the usage broker.

## Ownership

- `HostUsageProjectionRuntime` validates and presents one complete broker
  publication, stores canonical account selections, and keeps typed provider
  metric groups intact.
- Broker client and process helpers are re-exported for host consumers.
- Credential resolution, account discovery, provider calls, and shared state
  remain inside the broker service.

There is no client-side discovery or credential resolver in this crate.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | public `host` module | — |
| [`host.rs`](src/host.rs) | narrow broker and projection exports | [`host/tests.rs`](src/host/tests.rs) |
| [`host/projection/presentation.rs`](src/host/projection/presentation.rs) | projection validation, selection, and presentation | `host/tests/projection_presentation.rs` |

## Auth interaction limit

Unattended broker discovery does not resolve declarations that require operator
interaction. An `OpRef` backed source therefore has no relay capability until
the broker obtains matching credential material through a supported
non-interactive source. Its typed interaction-required issue is surfaced in the
broker projection. The current explicit auth-preparation flow covers Claude
Keychain credentials only; it does not prepare `OpRef` environment sources.

## How to verify

```sh
cargo nextest run -p jackin-usage-host-runtime
cargo clippy -p jackin-usage-host-runtime --all-targets -- -D warnings
```
