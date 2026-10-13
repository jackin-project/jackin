# jackin-runtime-attach-admission

Reconnect
admission checks.

## What this crate owns

- Admission
  (`admission`):
  `require_current_account_admission`,
  `validate_current_account_admission`,
  `require_current_instance_admission`,
  `validate_recorded_role_handle` —
  policy rechecks
  before reuse.
- Registration
  (`admission`):
  `refresh_registration_states`,
  `registration_state_for_admission`,
  `current_account_admission`,
  `mark_reconnect_admission_failure` —
  state refresh +
  failure marking.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`admission.rs`](src/admission.rs) | admission | hub `attach` suite |

## Public API

`admission::ReconnectAdmissionFailure`,
`admission::require_current_account_admission`,
`admission::require_current_instance_admission`,
`admission::validate_recorded_role_handle`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-attach-admission
cargo clippy -p jackin-runtime-attach-admission --all-targets -- -D warnings
```
