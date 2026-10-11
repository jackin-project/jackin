# jackin-runtime-prune-output

Formatted prune and cleanup
terminal output.

## What this crate owns

- Rows (`prune_output`):
  `start`, `PendingRow`,
  `pending_parts`.
- Status (`prune_output`):
  `section`, `ok`, `skip`,
  `failed`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`prune_output.rs`](src/prune_output.rs) | rows + status | `prune_output/tests.rs` |
| [`prune_output/`](src/prune_output/) | test suites | `prune_output/tests.rs` |

## Public API

`prune_output::start`,
`prune_output::PendingRow`,
`prune_output::section`,
`prune_output::ok`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-prune-output
cargo clippy -p jackin-runtime-prune-output --all-targets -- -D warnings
```
