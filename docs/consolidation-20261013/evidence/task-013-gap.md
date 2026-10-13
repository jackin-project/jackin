# TASK-013 gap audit

Read-only source audit. No product edits, no `cargo`, no `mbx`, no commit. This note does not approve TASK-013.

| Tree | Path | SHA |
|---|---|---|
| Integration | `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` | `ef5ca1df379ac31a52a61f5b1284deae17f5eebd` (`ef5ca1df` is HEAD) |
| Policy P | `/Users/donbeave/Projects/jackin-project/wt-baseline-P` | `fba69d60c2373156dc0dcecee8e085c83776966f` |
| Main M | `/Users/donbeave/Projects/jackin-project/wt-baseline-M` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| History | `/Users/donbeave/Projects/jackin-project/jackin` | inspected with `git show` / `git grep` only; no checkout |

Local history is missing `78e38612824c4e6d69ffb9d01a834a75537e7c5a`, `455526b92a2a4350476bb192455e5e3414f7ab9a`, and `cd3ced4189fb19624359da8c0eec5684ef1bacd8`. Those rows were judged from the integration tree and from the B002 plan text at `b1e08b880223594cceccf3ea44322d44e3cdf984`. Required nextest commands were not run.

## Binding

- **REJECT** the whole `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` tree. Its message says parent `0aa821a088e1bacf3d4d85a4c9faaa67faa85132` is a comparison baseline, not recovered ancestry, and that the tree preserves `abc5b11ca1b2aa0699947d54539cb291691189e4`. Unresolved markers remain in `crates/jackin-process/src/lib.rs` (lines 77, 183, 257, 440, 473, 650) and `crates/jackin-process/src/tests.rs` (line 231). The hunks are process capture-limit / output-reader variants. Current `output_limits` is already at `crates/core/jackin-process/src/lib.rs:177`. A search of the integration tree printed no `<<<<<<<` / `>>>>>>>` markers. Do not import markers or synthetic ancestry.
- **REJECT** permissive WAL. `1638522184ef45f0cd51fa5601a5e80c7fd89762` stops at an old-generation salt and keeps reading. Current validation refuses that fallback. Do not restore it.
- In-process broker fallback stays fail-closed. No edit below touches it.
- Do not mutate another repository.

## Obligations

### OMP database/WAL capture and journal reset-suffix handling — ALREADY_PRESENT

Capture reads `agent.db` and optional `agent.db-wal`, rejects a rollback journal, and refuses a pair that changes between two passes.

- `crates/adapters/jackin-omp-store/src/capture.rs:121` capture entry
- `crates/adapters/jackin-omp-store/src/capture.rs:130` two-pass read; mismatch returns unavailable (`:139`)
- `crates/adapters/jackin-omp-store/src/capture.rs:224` database validation, `:225` WAL validation, `:244` private WAL write
- `crates/adapters/jackin-omp-store/src/capture.rs:258` and `:312` refuse `-journal` / `-wal` / `-shm` sidecars on the materialized role database
- `crates/adapters/jackin-omp-store/src/capture.rs:634` and `:764` `reject_rollback_journal` (`agent.db-journal`)
- `crates/adapters/jackin-omp-store/src/capture.rs:652` WAL read
- `crates/adapters/jackin-omp-store/src/wal.rs:6` single-generation checksum through EOF; no fallback to an earlier commit
- `crates/adapters/jackin-omp-store/src/wal.rs:226` other-generation salt is `InvalidSalt`
- `crates/adapters/jackin-omp-store/src/wal/tests/case_01.rs:20` `rejects_a_stale_generation_suffix_instead_of_falling_back` covers `omp-reused-stale-suffix` and `omp-reused-uncommitted-stale-suffix`
- `crates/adapters/jackin-omp-store/src/capture/tests/case_01.rs:35` `source_writers_that_commit_or_checkpoint_between_capture_passes_are_rejected`

The two-pass compare is a change detector. It is not a claim that an optimistic reread is a SQLite backup.

### Ordered cleanup plus parent-directory durability — PORT

The durable order is already on the main paths:

- `crates/services/jackin-isolation/src/ref_transaction.rs:273` `Lock::cleanup` unlinks the lock, syncs that parent (`:286`), then unlinks the journal candidate (`:287`)
- `crates/services/jackin-isolation/src/ref_transaction.rs:722` `commit_into` syncs parents before dropping recovery names
- `crates/services/jackin-isolation/src/safe_remove.rs:240` `remove` unlinks, then `fsync`s the parent (`:242`)
- `crates/services/jackin-isolation/src/safe_remove.rs:284` `rename_into` syncs both parents (`:300`)
- `crates/core/jackin-config/src/persist.rs:890` `sync_parent`; staged write/delete commit calls it at `:910` and `:920`

`reclaim_abandoned_lock` still unlinks the journal candidate before the parent sync:

- `crates/services/jackin-isolation/src/ref_transaction.rs:401` unlink lock path
- `:402` unlink journal candidate
- `:403` sync parent

That is the same witness-loss window `Lock::cleanup` comments at `:283`. Smallest absent behavior: use that function's order on this one path.

### Stored agent identity restoration — ALREADY_PRESENT

Explicit restore still rejects launch overrides and requires the requested agent to equal the stored instance manifest.

- `crates/services/jackin-runtime/src/runtime/launch/launch_pipeline.rs:1731` `validate_explicit_restore_options`
- `:1751` read `InstanceManifest` and compare `requested_agent` to `stored_agent`
- `crates/services/jackin-runtime/src/runtime/launch/tests/case_16.rs:227` `explicit_restore_rejects_agent_different_from_stored_instance`
- `:260` `load_agent_attaches_explicit_restore_container_with_stored_agent_before_role_repo`

### Configuration migration predecessor versions and refusal of newer schemas — ALREADY_PRESENT

Predecessor chains and the historical fixture directories are present. A newer version is refused and is not migrated.

- `crates/core/jackin-config/src/migrations.rs:40` `CONFIG_MIGRATIONS`, including `v1alpha12` → current at `:118`
- `crates/core/jackin-config/src/migrations.rs:125` `WORKSPACE_MIGRATIONS`, including `v1alpha10` → current at `:187`
- `crates/core/jackin-config/src/versions.rs:11` current config `v1alpha13`; `:13` current workspace `v1alpha11`
- `crates/core/jackin-config/src/migrations.rs:795` `old_version > current` returns the upgrade error
- `crates/core/jackin-config/src/app_config/persist.rs:479` workspace newer-than-binary refusal
- `crates/core/jackin-config/src/app_config/persist.rs:520` in-memory read refusal (`UnsupportedVersion`)
- `crates/apps/jackin/tests/fixtures/migrations/config/from-v1alpha12/` `before.toml`, `after.toml`, `meta.toml`
- `crates/apps/jackin/tests/fixtures/migrations/workspace/from-v1alpha10/` `before.toml`, `after.toml`, `meta.toml`
- `crates/core/jackin-config/src/migrations/tests/case_03.rs:94` ambiguous breadcrumb input is rejected without writing
- `crates/core/jackin-config/src/app_config/persist/tests/case_03.rs:349` newer global version
- `crates/core/jackin-config/src/app_config/persist/tests/case_03.rs:372` newer workspace version

### Profile-proof creation/invalidation — ALREADY_PRESENT

Creation is the source identity plus material revision. Invalidation is a revision change when descriptor or payload bytes change. Do not add a new caller; the archived plan left an unconsumed consumer unselected.

- `crates/core/jackin-core/src/profile_material.rs:17` `ProfileCredentialSourceIdentity`
- `:24` `ProfileCredentialSourceMaterial`
- `:68` `profile_credential_source_identity`
- `:106` `profile_credential_material_revision`
- `crates/core/jackin-core/src/lib.rs:95` re-exports
- `crates/core/jackin-core/src/profile_material/tests/case_01.rs:7` descriptor-field identity
- `:107` `exact_strings_and_array_order_change_revision`
- `:201` `amp_revision_binds_only_exact_canonical_token` (changed token asserts `assert_ne!` at `:226`)

### CODEX_HOME discovery — ALREADY_PRESENT

Matches the reviewed follow-up `688057f40173d32dda04a55bff1e3868c219710d`: a nonempty value is the Codex directory; unset or empty uses `home/.codex`. Missing, relative-unresolvable, and non-directory values report `CODEX_HOME` and do not fall back. Do not restore the earlier empty-value error from `0556ce39b1abb9cd6b387583d932e1556ca9dfd4`.

- `crates/core/jackin-config/src/accounts/discovery.rs:218` scan comment and `CODEX_HOME` read
- `:231` `discover_default_accounts_with_codex_home`
- `:299` `codex_home_directory`
- `crates/core/jackin-config/src/accounts/discovery/tests/case_01.rs:63` missing explicit home does not fall back
- `:109` malformed explicit home does not fall back
- `:172` `empty_explicit_codex_home_uses_default_like_codex_cli`

### Projection-only Kimi capture — BLOCKED

Kimi locked capture still copies the whole source tree. The B002 row “Kimi full-tree snapshot / projection-only data” has no source SHA and no path list. Role provisioning already copies `config.toml`, `device_id`, and `credentials/`, but that is not the snapshot writer. Inventing a snapshot path set is out of scope.

- `crates/services/jackin-instance-agents/src/capture.rs:122` `Agent::Kimi` calls `snapshot_source`
- `crates/services/jackin-instance-credentials/src/auth_directory/stage.rs:126` `snapshot_source` calls `copy_tree`
- `crates/services/jackin-instance-agents/src/kimi.rs:40` provision comment (full `credentials/` tree)
- `crates/services/jackin-instance-agents/src/kimi.rs:136` provision copy of `KIMI_SYNC_FILES`
- `crates/services/jackin-instance-agents/src/kimi.rs:221` `KIMI_SYNC_FILES` = `config.toml`, `device_id`

## Edit list

One change:

1. In `crates/services/jackin-isolation/src/ref_transaction.rs`, `reclaim_abandoned_lock` (`:401`): after unlinking the lock path, `sync_directory` that parent before unlinking the journal candidate, then sync the journal directory. Match `Lock::cleanup` at `:282`.

No other file. Do not import `fea3ade`, conflict markers, synthetic parents, permissive WAL truncation, an in-process broker fallback, or a guessed Kimi snapshot set.

## Approval

TASK-013 is not approved. Nextest for `jackin-omp-store`, `jackin-config`, and `jackin-runtime` restore/migration/cleanup was not run. Independent review was not run.
