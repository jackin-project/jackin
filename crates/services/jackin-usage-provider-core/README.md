# jackin-usage-provider-core

Shared provider substrate for usage collection: snapshot cache, fetch
transport, outcome mapping, labels, and usage views. Consumed by the
per-vendor provider crates (`jackin-usage-provider-*`) and the host
usage services.

## What this crate owns

- Snapshot cache and keys (`cache`, `cache_keys`), fetch transport
  (`transport`, `http`, `io`, `credentials`), refresh waves (`refresh`).
- Outcome mapping (`outcome`, `fallback`, `surface`, `diagnostic`) and
  shared consts (`consts`).
- Labels and presentation (`labels`, `format`, `view`), omp attribution
  (`omp`), child-process telemetry (`process_telemetry`).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`cache.rs`](src/cache.rs) | snapshot cache | — |
| [`format.rs`](src/format.rs) · [`format/`](src/format) | value formatting | [`tests.rs`](src/format/tests.rs) |
| [`view.rs`](src/view.rs) · [`view/`](src/view) | usage view composition | — |
| [`refresh.rs`](src/refresh.rs) | refresh waves, rate limits | — |
| [`transport.rs`](src/transport.rs) · [`http.rs`](src/http.rs) · [`io.rs`](src/io.rs) | fetch transport | — |
| [`outcome.rs`](src/outcome.rs) · [`fallback.rs`](src/fallback.rs) · [`surface.rs`](src/surface.rs) · [`diagnostic.rs`](src/diagnostic.rs) | outcome mapping | — |
| [`labels.rs`](src/labels.rs) | display/storage labels | — |
| [`cache_keys.rs`](src/cache_keys.rs) · [`consts.rs`](src/consts.rs) · [`credentials.rs`](src/credentials.rs) | keys, consts, credential files | — |
| [`omp.rs`](src/omp.rs) · [`omp/`](src/omp) | omp attribution adapter | [`tests.rs`](src/omp/tests.rs) |
| [`process_telemetry.rs`](src/process_telemetry.rs) | child-process telemetry | — |

## Public API

Cache, labels, views, and fetch helpers consumed by
`jackin-usage-provider-*`, `jackin-usage`, and downstream crates.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-core
cargo clippy -p jackin-usage-provider-core --all-targets -- -D warnings
```
