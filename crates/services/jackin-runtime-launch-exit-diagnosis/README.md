# jackin-runtime-launch-exit-diagnosis

Exit diagnosis
for premature exits.

## What this crate owns

- Diagnose
  (`exit_diagnosis`):
  `diagnose_premature_exit`,
  `diagnose_with_state`,
  `ExitPhase` — pre/post-attach
  exit triage.
- Outcome
  (`exit_diagnosis`):
  `inspect_attach_outcome`,
  `attach_failure_error`,
  `is_known_socket_close` —
  post-attach state.
- Identity
  (`exit_diagnosis`):
  `diagnose_premature_exit_by_id`,
  `inspect_attach_outcome_by_id`,
  `diagnose_with_state_by_id` —
  immutable-ID variants.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`exit_diagnosis.rs`](src/exit_diagnosis.rs) | diagnose + outcome | — |

## Public API

`exit_diagnosis::ExitPhase`,
`exit_diagnosis::diagnose_premature_exit`,
`exit_diagnosis::diagnose_premature_exit_by_id`,
`exit_diagnosis::diagnose_with_state`,
`exit_diagnosis::diagnose_with_state_by_id`,
`exit_diagnosis::attach_failure_error`,
`exit_diagnosis::is_known_socket_close`,
`exit_diagnosis::inspect_attach_outcome`,
`exit_diagnosis::inspect_attach_outcome_by_id`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-exit-diagnosis
cargo clippy -p jackin-runtime-launch-exit-diagnosis --all-targets -- -D warnings
```
