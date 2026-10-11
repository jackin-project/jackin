# jackin-runtime-spin-wait

Async spinner-wait helper for
polling operations: braille
spinner on stderr, silenced while
the rich launch cockpit owns the
terminal.

## What this crate owns

- Wait (`spin_wait`): `spin_wait`
  (fixed interval) and
  `spin_wait_ramped` (exponential
  backoff to cap) over an async
  poll closure.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`spin_wait.rs`](src/spin_wait.rs) | spinner + waits | `spin_wait/tests/` |

## Public API

`spin_wait::spin_wait`,
`spin_wait::spin_wait_ramped`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-spin-wait
cargo clippy -p jackin-runtime-spin-wait --all-targets -- -D warnings
```
