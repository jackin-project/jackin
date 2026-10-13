# jackin-runtime-apple-container-ensure-absent

Jackin apple-container
purge-absence guard
helper.

## What this crate owns

- Purge-absence guard
  (`ensure_absent`):
  `ensure_absent_for_purge_with` —
  bail while the named
  container still exists,
  through an injected
  client (S7 split 121).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`ensure_absent.rs`](src/ensure_absent.rs) | Purge-absence guard helper | hub `backend` purge path (no dedicated suite) |

## Public API

`ensure_absent::ensure_absent_for_purge_with`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-ensure-absent
cargo clippy -p jackin-runtime-apple-container-ensure-absent --all-targets -- -D warnings
```
