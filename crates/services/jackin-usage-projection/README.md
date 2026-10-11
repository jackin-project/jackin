# jackin-usage-projection

Surface-neutral canonical usage
projection: build the immutable V1
publication from discovery plus
broker generations.

## What this crate owns

- Canonical (`canonical`):
  `build_canonical_projection`
  over the account catalog.
- Accounts (`account`):
  per-account projection with
  generation metadata and issues.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`canonical.rs`](src/canonical.rs) | V1 builder | `tests/` |
| [`account.rs`](src/account.rs) | accounts | `tests/` |

## Public API

`build_canonical_projection`.

## How to verify

```sh
cargo nextest run -p jackin-usage-projection
cargo clippy -p jackin-usage-projection --all-targets -- -D warnings
```
