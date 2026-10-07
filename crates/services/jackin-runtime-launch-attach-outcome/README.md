# jackin-runtime-launch-attach-outcome

Instance status
and attach-outcome
persistence.

## What this crate owns

- Attach outcome
  (`attach_outcome`):
  `write_instance_status`,
  `write_instance_attach_outcome`,
  `record_instance_attach_outcome`,
  `format_attach_outcome` —
  status writes and
  outcome records
  against recorded
  instance state.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`attach_outcome.rs`](src/attach_outcome.rs) | status + outcome | hub `launch` suite (`case_26`) |

## Public API

`attach_outcome::write_instance_status`,
`attach_outcome::write_instance_attach_outcome`,
`attach_outcome::record_instance_attach_outcome`,
`attach_outcome::format_attach_outcome`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-attach-outcome
cargo clippy -p jackin-runtime-launch-attach-outcome --all-targets -- -D warnings
```
