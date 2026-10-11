# jackin-runtime-launch-restore

Restore candidate
discovery, choice,
and resolution for
launch.

## What this crate owns

- Candidates
  (`restore`):
  `related_restore_candidates`,
  `matching_instance_manifests`,
  `present_restore_choice`,
  `related_restore_load_options`,
  `write_preserved_status_if_applicable`,
  `preserved_instance_status` —
  same-role + related
  candidate discovery,
  the rich launch
  dialog, and
  preserved-status
  persistence.
- Resolution
  (`restore_resolve`):
  `RestoreResolution`,
  `resolve_restore_candidate`,
  `admit_restore`,
  `EarlyCurrentRestoreScan` —
  mapping Docker
  inspect state to a
  launch decision.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`restore.rs`](src/restore.rs) | candidates + dialog + status | hub `launch::restore` suite (`restore/tests.rs`) + hub `launch` suite |
| [`restore_resolve.rs`](src/restore_resolve.rs) | resolution engine | hub `launch` suite |

## Public API

`restore::present_restore_choice`,
`restore::related_restore_candidates`,
`restore::matching_instance_manifests`,
`restore::matching_current_role_manifests`,
`restore::write_preserved_status_if_applicable`,
`restore::preserved_instance_status`,
`restore_resolve::RestoreResolution`,
`restore_resolve::resolve_restore_candidate`,
`restore_resolve::resolve_restore_candidate_reusing_early`,
`restore_resolve::admit_restore`,
`restore_resolve::EarlyCurrentRestoreScan`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-restore
cargo clippy -p jackin-runtime-launch-restore --all-targets -- -D warnings
```
