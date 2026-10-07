# jackin-usage-discovery

Rust-owned host account-source
discovery: scan authorized sources,
validate bindings, map bindings to
broker capabilities.

## What this crate owns

- Scan (`scan`, `ownership`,
  `accumulate`, `source`):
  authorized-source enumeration and
  pre-deduplication.
- Providers (`providers`,
  `profiles`, `identity`,
  `catalog`): per-surface profile
  readers and credential binding.
- Validation (`validate`,
  `refresh`, `scope`, `issues`):
  binding validation and refresh
  dispatch.
- Capabilities (`capabilities`):
  binding-to-capability mapping
  (`capability_for_binding`,
  `usage_broker_capabilities`,
  `usage_catalog_entries`).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`scan.rs`](src/scan.rs) | enumeration | `tests/` |
| [`validate.rs`](src/validate.rs) | validation | `tests/` |
| [`refresh.rs`](src/refresh.rs) | refresh | `tests/` |
| [`capabilities.rs`](src/capabilities.rs) | mapping | `tests/` |
| others | providers/scope/issues | `tests/` |

## Public API

`discover_usage_sources`,
`validate_usage_sources`,
`ValidatedUsageDiscovery`, and the
capability-mapping functions.

## How to verify

```sh
cargo nextest run -p jackin-usage-discovery
cargo clippy -p jackin-usage-discovery --all-targets -- -D warnings
```
