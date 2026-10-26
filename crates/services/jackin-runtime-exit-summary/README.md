# jackin-runtime-exit-summary

Exit "still running"
summary data for
diagnostics.

## What this crate owns

- Summary (`exit_summary`):
  `summary` — headline +
  rows from the running set
  and the instance index.
- Privacy (`exit_summary`):
  saved workspaces list by
  role; ad-hoc folders
  collapse to a count.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`exit_summary.rs`](src/exit_summary.rs) | headline + rows | — |

## Public API

`exit_summary::summary`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-exit-summary --no-tests warn
cargo clippy -p jackin-runtime-exit-summary --all-targets -- -D warnings
```
