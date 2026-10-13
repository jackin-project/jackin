# jackin-usage-host-glance

Host label and glance-row render
helpers: provider glance rows,
account descriptors, driving
buckets, and status-bar rank keys.

## What this crate owns

- Rows (`render`):
  `build_provider_glance_row`,
  `account_descriptor`,
  `glance_bucket`, and the
  `DrivingBucket` selection.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`render.rs`](src/render.rs) | builders | (via `jackin-usage`) |

## Public API

`build_provider_glance_row`,
`account_descriptor`, and the
status-bar helpers.

## How to verify

```sh
cargo nextest run -p jackin-usage-host-glance
cargo clippy -p jackin-usage-host-glance --all-targets -- -D warnings
```
