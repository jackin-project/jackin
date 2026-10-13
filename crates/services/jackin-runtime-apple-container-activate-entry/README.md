# jackin-runtime-apple-container-activate-entry

Jackin apple-container
started-entry
activation.

## What this crate owns

- Started-entry
  activation
  (`activate_entry`):
  `activate_started_entry` —
  bail with the
  capabilities hint on
  a failed `container
  run`, else activate
  the running launch
  entry claim (S7 split 113).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`activate_entry.rs`](src/activate_entry.rs) | started-entry activation | hub `apple_container` launch path (no dedicated suite) |

## Public API

`activate_entry::activate_started_entry`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-activate-entry
cargo clippy -p jackin-runtime-apple-container-activate-entry --all-targets -- -D warnings
```
