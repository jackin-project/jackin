# jackin-runtime-apple-container-check-dns

Jackin apple-container
DNS health check.

## What this crate owns

- DNS check
  (`check_dns`):
  `check_dns` —
  run an `nslookup`
  probe after attach
  and warn on the
  sleep/wake DNS
  hiccup (S7 split 118).

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`check_dns.rs`](src/check_dns.rs) | DNS health check | hub `apple_container` launch path (no dedicated suite) |

## Public API

`check_dns::check_dns`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-check-dns
cargo clippy -p jackin-runtime-apple-container-check-dns --all-targets -- -D warnings
```
