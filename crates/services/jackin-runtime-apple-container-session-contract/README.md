# jackin-runtime-apple-container-session-contract

Jackin apple-container
session contract
printer.

## What this crate owns

- Session contract
  (`session_contract`):
  `print_session_contract` —
  print the security
  boundary summary
  before interactive
  attach (S7 split 119).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`session_contract.rs`](src/session_contract.rs) | Session contract printer | hub `apple_container` launch path (no dedicated suite) |

## Public API

`session_contract::print_session_contract`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-session-contract
cargo clippy -p jackin-runtime-apple-container-session-contract --all-targets -- -D warnings
```
