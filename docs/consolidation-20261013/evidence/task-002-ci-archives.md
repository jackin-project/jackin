# TASK-002 CI archive evidence

Read-only inspection of `/Users/donbeave/Projects/jackin-project/jackin` on 2026-10-13. No checkout, commit, reset, build, or live service call. Jackin source was not modified.

- M = `1c3dd1be4a3b1ed518133ec3899c084c792370ff`
- P = `fba69d60c2373156dc0dcecee8e085c83776966f` (destination trees only; the 366 commits ahead of M were not reviewed)
- Unique range: commits reachable from each tip and not reachable from M (`git rev-list M..TIP`)

M is not an ancestor of any of these tips. Each tip's first parent outside the unique range is reachable from both M and P.

| Tip | SHA | Unique commits |
| --- | --- | ---: |
| B005 | `1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d` | 1 |
| B010 | `6d65e649855f98f133f58f605727b098b5d4eabb` | 3 |
| B012 | `14bd1471616056e07c81c59d59eafc0963b16865` | 2 |
| B020 | `c7b8a95856cce6f5e8442f0e6fafcf025a089132` | 1 |
| B039 | `41dd148e514d1bf3f84401f0ecbf8e87b58bb261` | 1 |
| Union |  | 8 |

## Unique commits

### B005 — 1 commit

`1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d`
Subject: `feat(xtask): add durable CI evidence ledger`
Parents: `310e644832193232344ca99838ccf599f28faf53` (reachable from M and P)
Body names cherry-pick source `0c656c4159c4d6167e8d804a938b6419efa5f6d1`.
Tree: `20aec017db4f485aebeec259e9fe6ba33f64f579`. Stable patch-id `6ab703dac1ff54c2e763b428ce14121302dc36ad`.

Changed paths:

- `Cargo.lock`
- `crates/jackin-xtask/Cargo.toml`
- `crates/jackin-xtask/src/ci_evidence.rs` (`SCHEMA` = 4)
- `crates/jackin-xtask/src/ci_evidence/github.rs`
- `crates/jackin-xtask/src/ci_evidence/ledger.rs`
- `crates/jackin-xtask/src/ci_evidence/rollup.rs`
- `crates/jackin-xtask/src/ci_evidence/storage.rs`
- `crates/jackin-xtask/src/ci_evidence/tests.rs`
- `crates/jackin-xtask/src/ci_evidence/validation.rs`
- `crates/jackin-xtask/src/main.rs`

### B010 — 3 commits

Stash created on `65e9dfbdba1066c29fa3a3ea88a7930cb833152b` (`docs(ci): probe C completes §14 bullet 1 (#1029)`), which is reachable from M and P. Committer time of all three unique commits is `2026-09-21T19:11:16+07:00`.

`9dcdd23211facd8fbf82f9031b1e9e6dc4b3595c`
Subject: `untracked files on fix/clippy-affected-index: 65e9dfbd docs(ci): probe C completes §14 bullet 1 (#1029)`
Parents: none
Tree: `4b825dc642cb6eb9a060e54bf8d69288fbee4904` (Git empty tree)
Changed paths: none

`0624d32807427aca808eecfa812233154c9a517b`
Subject: `index on fix/clippy-affected-index: 65e9dfbd docs(ci): probe C completes §14 bullet 1 (#1029)`
Parents: `65e9dfbdba1066c29fa3a3ea88a7930cb833152b`
Tree: `bcbf211d6682fad291d093257246d8300214cf25`, identical to the parent tree
Changed paths: none (`git diff 65e9dfbd 0624d328` is empty)

`6d65e649855f98f133f58f605727b098b5d4eabb`
Subject: `On fix/clippy-affected-index: hk`
Parents: `65e9dfbdba1066c29fa3a3ea88a7930cb833152b`, `0624d32807427aca808eecfa812233154c9a517b`, `9dcdd23211facd8fbf82f9031b1e9e6dc4b3595c`
`git diff-tree` on the merge commit lists no files. The worktree payload versus the first parent (and versus the identical index parent) is:

- `crates/jackin-xtask/src/clippy_affected.rs`
- `crates/jackin-xtask/src/clippy_affected/tests.rs`

Those two blobs are identical to M (`badfd24764c81fff810feff5a8cf85f2b7bf54be`, `9f6994cff19d56c394ee72da17872aa4b01dd63d`). The staged index captured no extra delta. The staged-only behavior is in the worktree source, not in a distinct index tree.

### B012 — 2 commits

`20acaad879e89f4e5bb7d89fa095d01ad900db53`
Subject: `feat(xtask): add durable CI evidence ledger`
Parents: `310e644832193232344ca99838ccf599f28faf53`
Tree `20aec017db4f485aebeec259e9fe6ba33f64f579` and stable patch-id `6ab703dac1ff54c2e763b428ce14121302dc36ad` match `1c42a33b`. Committer time is 7 seconds later (`2026-09-29T17:52:12+07:00`). Same changed paths as B005. No additional behavior.

`14bd1471616056e07c81c59d59eafc0963b16865`
Subject: `fix(ci): harden evidence qualification`
Parents: `20acaad879e89f4e5bb7d89fa095d01ad900db53`
Body names cherry-pick source `8a8c2db9181df7d2bb51b964fd668982835886e9`. `SCHEMA` = 5.

Changed paths:

- `crates/jackin-xtask/src/ci_evidence.rs`
- `crates/jackin-xtask/src/ci_evidence/github.rs`
- `crates/jackin-xtask/src/ci_evidence/ledger.rs`
- `crates/jackin-xtask/src/ci_evidence/rollup.rs`
- `crates/jackin-xtask/src/ci_evidence/storage.rs`
- `crates/jackin-xtask/src/ci_evidence/tests.rs`
- `crates/jackin-xtask/src/ci_evidence/validation.rs`

### B020 — 1 commit

`c7b8a95856cce6f5e8442f0e6fafcf025a089132`
Subject: `untracked files on codex/ci-evidence-repair-current-main-20260923: a65f5d75c fix(usage): route Claude API keys away from OAuth adapter (#1092)`
Parents: none. This is an untracked-files root, not a full checkout. `SCHEMA` = 5. `ledger.rs` and `rollup.rs` blobs match `14bd1471`. The other evidence modules differ; `validation.rs` adds generated-ownership and live API binding helpers.

Changed paths (all adds):

- `.github/workflows/ci-evidence.yml`
- `.github/workflows/ci-push-head-ledger.yml`
- `crates/jackin-xtask/src/ci_evidence.rs`
- `crates/jackin-xtask/src/ci_evidence/github.rs`
- `crates/jackin-xtask/src/ci_evidence/ledger.rs`
- `crates/jackin-xtask/src/ci_evidence/rollup.rs`
- `crates/jackin-xtask/src/ci_evidence/storage.rs`
- `crates/jackin-xtask/src/ci_evidence/tests.rs`
- `crates/jackin-xtask/src/ci_evidence/validation.rs`

Both workflow files set `permissions: contents: read` and `cancel-in-progress: true`.

### B039 — 1 commit

`41dd148e514d1bf3f84401f0ecbf8e87b58bb261`
Subject: `WIP recovery snapshot: Jackin sccache successor changes preserved 2026-10-06`
Parents: `9598c3662fd6befa790d3a5ee0dae99b80565e70` (reachable from M and P)

Changed paths:

- `.github-gen/velnor-workflow.toml`
- `.github/ci/.github-actions-generator-state`
- `.github/workflows/desktop-merge.yml`
- `crates/jackin-xtask/src/ci/tests.rs`
- `mise.toml`

## Behavior judgments

Destination path note: M keeps `crates/jackin-xtask/`. P keeps the same modules under `crates/tools/jackin-xtask/`. These blobs are identical between M and P: `ledger.rs`, `github.rs`, `rollup.rs`, `storage.rs`, `producer.rs`, `output.rs`. `ci_evidence.rs` differs by deletion of `MISE_PATH`. `validation.rs` differs by dropping the `mise.toml` task-`run` cross-check and binding the workflow step command directly.

### Durable push-head / producer / artifact qualification

Present in `1c42a33` (schema 4), hardened in `14bd1471` (schema 5), and carried in the `c7b8a958` untracked tree (schema 5 plus generated-state checks). `20acaad8` adds no further rule.

Archive rules with file evidence:

- Expected obligations are independent of observed runs. `1c42a33:crates/jackin-xtask/src/ci_evidence.rs` records a missing workflow as missing. `rollup_counts_missing_and_does_not_recode_cancelled_rerun` keeps a first-attempt cancellation when a later attempt is success.
- Cancellation is its own class (`github.rs` matches `cancelled` / `canceled`). A failed or cancelled run stays non-green (module comment in `ci_evidence.rs`).
- Nonterminal rows stay non-terminal. `14bd1471` comment: a later collection may turn a nonterminal row terminal. `active_attempt_has_no_terminal_timing_verdict` uses status `in_progress`.
- Conflicting terminal observations stay attached (`storage.rs` `ConflictingTerminalObservation`; test `conflict_marker_requires_raw_conflicting_observations`).
- `14bd1471:crates/jackin-xtask/src/ci_evidence/ledger.rs` requires a strictly earlier boundary predecessor whose head matches the first before SHA, and rejects a chain gap without an observed-head fallback.
- `fixture_denominator_cannot_qualify_green` asserts `!rollup.green_claim_qualified` and `require_qualified(&rollup).is_err()`.
- Artifact bytes are digest-bound (`retained_push_head_artifact_bytes_are_digest_bound`).
- No `first_parent` / `FirstParent` symbol in these evidence trees.

P already contains the visibility and binding rules, at schema 7, and they are equal to M for the shared modules:

- `crates/tools/jackin-xtask/src/ci_evidence.rs` lines 1–11 and 34: explicit expected set, never-started workflow stays `missing`, collector validates a producer artifact, `SCHEMA` = 7. Line 88: a failed run stays non-green when infrastructure or cancelled.
- `crates/tools/jackin-xtask/src/ci_evidence/rollup.rs` lines 1–61: only `is_first_attempt` rows enter the cohort; a missing obligation is `OutcomeClass::Missing`; cancellation, infrastructure, and data-quality stay separate counters. Line 128 sets `status: "advisory"`.
- `crates/tools/jackin-xtask/src/ci_evidence/tests/case_03.rs` `rollup_counts_missing_and_does_not_recode_cancelled_rerun` (line 187): first attempt cancellation plus a later success still yields `ci.cancellation == 1`, `ci.missing == 1`, and `end_to_end == OutcomeClass::Cancellation`.
- `crates/tools/jackin-xtask/src/ci_evidence/tests/case_03.rs` `active_attempt_has_no_terminal_timing_verdict` (line 7): `in_progress` classifies as `DataQuality` with `completed_at == None`.
- `crates/tools/jackin-xtask/src/ci_evidence/storage.rs` `is_terminal` (line 240) is true only for status `completed`. `merge_attempt_observation` (line 190) keeps raw observations and, on more than one terminal observation, sets `ConflictingTerminalObservation`.
- `crates/tools/jackin-xtask/src/ci_evidence/github.rs` lines 878–881 map cancelled status or conclusion to `OutcomeClass::Cancellation`.
- `crates/tools/jackin-xtask/src/ci_evidence/ledger.rs` lines 42–171 load the push-head ledger, reject an empty workflow id set, and on a present predecessor require a strictly earlier run whose head matches the first before SHA (lines 549–560). Artifact ZIP size and API digest must match (lines 344–347). `PUSH_HEAD_LEDGER_SCHEMA` = 2.
- `crates/tools/jackin-xtask/src/ci_evidence/tests/case_04.rs` `push_head_chain_gap_is_rejected_without_observed_head_fallback` (line 7) still rejects a coverage gap.
- `crates/tools/jackin-xtask/src/ci_evidence/producer.rs` lines 22, 91, and 178 reject a non-push event, a producer not bound to the generated main push workflow, and an API run that does not match the active producer attempt.
- `git grep first_parent` / `FirstParent` under P `ci_evidence` is empty. These five tips do not contain a first-parent denominator.

Not marked superseded:

- Fail-closed green qualification is absent from both M and P. `git grep green_claim_qualified` and `require_qualified` over M `crates/jackin-xtask/src/ci_evidence` and P `crates/tools/jackin-xtask/src/ci_evidence` return no matches. P `fixture_denominator_remains_advisory`, `local_collection_remains_advisory`, and `scheduled_provenance_is_reported_without_a_semantic_claim` only assert `rollup.status == "advisory"`. That is a different gate from `14bd1471` `require_qualified`.
- An always-required predecessor is absent from P. `ledger.rs` lines 136–144, when no predecessor run exists, set `DenominatorBoundary::Bootstrap` and move `window.since` to the first in-window push. The same-named test at `case_04.rs:35` now rejects `DenominatorBoundary::Fixture` (`missing its boundary proof`), not a missing predecessor.
- Live `.github/workflows/ci-evidence.yml` and `ci-push-head-ledger.yml` are absent from both M and P (`git cat-file -e` fails). P keeps newer fixture copies at `crates/tools/jackin-xtask/src/ci_evidence/fixtures/`. Those fixtures still declare `permissions: contents: read` and `cancel-in-progress: true`. Relative to `c7b8a958` they change timeout 20 to 30, add `continue-on-error: true`, `fetch-depth: 0` on the ledger checkout, and replace `mise run …` with `MISE_AUTO_INSTALL=false mise exec -- mbx +1.99.0 xtask …`. Restoring the `c7b8a958` workflow bytes would undo that fixture delta. P `validation.rs` `validate_generated_evidence_workflows` (line 781) still describes those workflow files; that function is not a live workflow in the P tree.

`1c42a33` schema 4 lacks the schema-5 predecessor, digest, and `require_qualified` tests that `14bd1471` adds (`push_head_window_requires_a_verified_boundary_predecessor`, `retained_push_head_artifact_bytes_are_digest_bound`, `absent_evidence_workflows_fail_closed`). Schema 4 is the weaker ancestor, not a second design to prefer over P's schema 7 visibility rules.

### Staged-only affected-crate selection

Present only in `6d65e649` (the two clippy files). The index and untracked parents contribute no bytes.

The worktree patch unions HEAD↔index and index↔worktree. `union_lock_changes` / `union_workspace_dependency_changes` return `None` when either pair is unparseable or structurally ambiguous. Tests: comment-only stays empty, staged-only version bump selects `serde`, staged-only structural manifest edit widens, the two pairs merge, and either broken pair widens.

P contains that behavior. `crates/tools/jackin-xtask/src/clippy_affected.rs` is the M file plus four line edits: the hook comment says `cargo xtask clippy-affected`, and `NIGHTLY_ONLY_NESTED` is `crates/tools/jackin-lints`. Lines 353–400 still define `worktree_lock_packages`, `index_file`, `union_lock_changes`, and `union_workspace_dependency_changes`, including the comment that a staged-only edit with the worktree reverted to HEAD must select. Tests of the same names are in `crates/tools/jackin-xtask/src/clippy_affected/tests/case_02.rs` (`manifest_union_staged_only_breaking_edit_widens` at line 150, `lock_union_staged_only_version_bump_selects` at line 160, `unions_merge_head_index_and_index_worktree_pairs` at line 170, `unions_widen_when_either_pair_is_unparseable` at line 192).

### Tool-wrapper isolation

Present in `41dd148e` `mise.toml` and `crates/jackin-xtask/src/ci/tests.rs`.

`desktop-xcframework` and `desktop-ci` define `run_with_sccache`. It runs the command unchanged when `command -v sccache` fails. Otherwise it sets `RUSTC_WRAPPER=sccache` only for that command. `SCCACHE_GHA_ENABLED` stays `off` unless `SCCACHE_GHA_ENABLED=true` and both `ACTIONS_CACHE_URL` and `ACTIONS_RUNTIME_TOKEN` are set. The Velnor comment says profile and unit env are job-level and would poison the installer, so wrapper env stays in the task.

Tests `sccache_is_absent_from_installer_environment` and `sccache_bootstrap_is_task_scoped_after_tool_install` require the installer prefix to lack `RUSTC_WRAPPER` / `SCCACHE_GHA_ENABLED=true` / `CARGO_INCREMENTAL=0`, and require the Mise setup step before the Cargo task. `sccache_backend_modes_smoke_without_github_runtime` returns immediately when `sccache` or `rustc` is missing, so a pass of that test is not execution evidence.

Also in this commit, and not one of the four behaviors: `desktop-merge.yml` gains `pull_request` and sets `cancel-in-progress` only for `pull_request`. The generator-state file updates the config digest and the `desktop-merge.yml` output digest. P has no `.github/workflows/desktop-merge.yml`. P `crates/tools/jackin-xtask/src/ci/tests.rs` lines 128–132 say that workflow, `desktop-scheduled.yml`, `ci-main.yml`, `ci-pr.yml`, and `ci-unit-swift.yml` were deleted. That comment does not reimplement the pull-request trigger.

P does not contain `run_with_sccache` or `command -v sccache`. `git grep sccache` on P `mise.toml` and `native/mise.toml` is empty. P `crates/tools/jackin-xtask/src/ci/tests.rs` lines 87–125 `assert_no_sccache_env`, `sccache_is_absent_from_generated_workflow_environment`, and `sccache_is_absent_from_installer_environment` forbid `RUSTC_WRAPPER: sccache`, `RUSTC_WRAPPER=sccache`, `SCCACHE_GHA_ENABLED: "true"`, and `CARGO_INCREMENTAL` exports in `.github/workflows/ci.yml` and in the text before the Mise setup step. M still pins `"cargo:sccache" = "0.16.0"` in `mise.toml` line 24 and does not set `RUSTC_WRAPPER` in that file; the only M `RUSTC_WRAPPER` code hit is the same absence assertion in `crates/jackin-xtask/src/ci/tests.rs`. M research and plan docs describe historical or proposed wrapper wiring; they are not the `41dd148e` task-local implementation.

The leak prohibition is present at P as absence, which is stronger than a task-local export for the generated workflow and installer prefix. The positive rule that a wrapper binary must exist before `RUSTC_WRAPPER` is set is absent from P, because P has no such wrapper. Not marked superseded.

### Cache reuse requiring exact semantic inputs

None of the eight unique commits implement cache reuse keyed to exact semantic inputs. `git grep` for `semantic input`, `cache reuse`, and `exact semantic` under P `crates/tools/jackin-xtask`, `mise.toml`, and `.github`, and the same paths at M, is empty.

`41dd148e` gates the sccache GHA backend on URL and token presence. That is a credential check, not a semantic input identity. P fixture `cache: true` / `cache_save` expressions key off event name and ref, not semantic inputs. Old global sccache activation is not in this unique payload: `41dd148e` moves the wrapper into the task and says job-level profile env would poison the installer. P's absence tests above are the destination rejection of those env strings in `ci.yml` and the installer prefix. Do not restore a global `RUSTC_WRAPPER` / `SCCACHE_GHA_ENABLED=true` job environment from older history to satisfy this gap.

## Probe debris and other non-behaviors

Proven empty:

- `9dcdd23211facd8fbf82f9031b1e9e6dc4b3595c` is the empty tree. Subject is the stash untracked-files label on the probe C commit. No files, no behavior.
- `0624d32807427aca808eecfa812233154c9a517b` has the same tree as `65e9dfbd`, already reachable from M and P. Subject is the stash index label on the same probe C commit. No staged delta.

Not debris:

- `6d65e649` carries the clippy union even though its stash parents are empty.
- `20acaad8` is a duplicate tree of `1c42a33`, not an empty commit and not a second behavior.
- `c7b8a958` is an untracked root with real schema-5 sources and generated workflows. The workflow bytes are stale relative to the P fixtures (diff above) and are not live workflows at M or P. The Rust qualification overlap is judged above. `contents: read` remains in the P fixtures, so that permission weakness is not fixed by P.
- `41dd148e` is a recovery snapshot with real task and workflow edits. Its `sccache_backend_modes_smoke_without_github_runtime` early return is not proof the smoke ran. That does not make the task-local wrapper empty.

## Still absent from P

- Fail-closed `green_claim_qualified` / `require_qualified` (also absent from M).
- Unconditional missing-predecessor failure. P bootstraps in `crates/tools/jackin-xtask/src/ci_evidence/ledger.rs` lines 136–144.
- Task-scoped wrapper provision-before-use (`run_with_sccache`, `command -v sccache`). P forbids wrapper env in `ci.yml` and the installer prefix instead.
- Cache reuse that requires exact semantic inputs (also absent from M).

Present at P, with the citations above: push-head / producer / artifact binding; missing, cancelled, in-progress, conflicting, and failed first attempts stay visible; staged-only affected-crate union. No first-parent denominator is in these tips or in P `ci_evidence`.
