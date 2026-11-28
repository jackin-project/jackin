# Retire verification: preserve/t3 refs 17 and 18

Verifier: Haiku 5.5 (effort high), read-only. Fetch run with the HTTPS rewrite; no ref, worktree or repo file was changed.

`origin/main` at verification time: `f151478146fb335ab4540ed058e3c430464335aa`. It contains #1321 merge `51f2d80885c58e0edcd9dc29d03bd61f07f0f4bc` (P08 landed as part of batch 1; commit body cites #1309).

Plan: `<workspace>/jackin/jackin-branch-porting-plan.md`, section 5 (P08, line 190) and section 3 (preserve rules).

## Verdicts

| Ref | SHA | Verdict |
|---|---|---|
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/17` | `9156f83145b39ff0f73d5a476a17847bc80c7c43` | **SAFE** |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/18` | `1dd6b8e4ed5f7716662a74d184c9d382b9702ab9` | **SAFE** |

No VALUE. No UNCERTAIN. Refs 17 and 18 are the two t3 refs not covered by the sibling report (`retire-verify-t3-body.md`, which covers 0-2, 10-16, 19-29).

## Key findings

1. **The refs do not carry P08's fix.** P08 (path-manifest refresh, `path_role_doc`) is absent from both refs. The ref versions of `crates/jackin-tui/src/observe.rs` `role_docs` are identical to the pre-#1321 main version, apart from unrelated later main changes.
2. **P08 did not take anything from these refs.** The plan's manifest rows (plan lines 564-565) say `RETIRE-COVERED; no distinct product port` and warn "do not confuse with local-manifest freshness P08". The brief's premise that these refs "served only P08" does not match the plan.
3. **The only unique content is the Git-role picker work**, which predates #1321 on main. Each ref's tip (`9156f83`, `1dd6b8e`, same tree, same parent `4691c00896`) changes two files versus its parent: `crates/jackin-tui/src/observe.rs` (+31/-14, Git-sourced role docs loaded through the catalog instead of `RoleNotLoaded`) and `crates/jackin-tui/tests/launch/picker.rs` (+57, new test).

## Commit and path accounting

| Item | Ref 17 | Ref 18 |
|---|---|---|
| Tip | `9156f83` | `1dd6b8e` |
| Tip tree | `c6278e9c9a0882141944b305382cbf99f03c7936` | identical |
| Commits reachable only from this ref (not from any other remote ref) | 1 (its tip) | 1 (its tip) |
| Commits not on `origin/main` | 1045 (not meaningful: main is a squash rewrite, not a descendant) | 1045 |
| Tree diff vs `origin/main` | 898 files: 43 A, 469 M, 1 R054, 385 D (`git diff --name-status`) | identical tree, identical counts |
| Tip vs parent `4691c00896` | 2 files, +86/-16 | identical |
| 17 vs 18 | identical tree; tips differ only by commit metadata | |

Parent `4691c00896` ("test: migrate Jackin identity coverage to protobuf baselines (#1270)") is the "T-A" lineage in the sibling report. It is an ancestor of both kept checkpoints `origin/codex/openshell-verified-checkpoint-20261108` and `origin/codex/openshell-integration-checkpoint-20261108-122311` (checked with `merge-base --is-ancestor`). Deleting refs 17 and 18 therefore loses no commit.

## Invariant check (tip content vs origin/main)

| Ref-side behavior | On origin/main | Evidence |
|---|---|---|
| Path-manifest refresh and cached failure reload (P08, not in refs) | Yes, added by #1321 | `crates/jackin-tui/src/observe.rs:364-384` `path_role_doc`; `role_docs` filter `:391-394` (Path always reloaded; Git only when cache missing or source changed); `:396-400` previous-doc reuse |
| P08 tests added by #1321 | Yes | `crates/jackin-tui/tests/console/roles.rs:397` `a_local_role_manifest_edited_outside_the_console_shows_after_a_refresh` (same path, valid, invalid, valid, sibling unaffected); `:433` `a_manifest_edited_outside_the_console_clears_only_its_own_check`; `:476` `a_manifest_that_becomes_invalid_outside_the_console_clears_its_check`; `:495` `a_role_repointed_to_an_equal_manifest_loses_the_check_of_the_old_source` |
| Git-sourced role docs loaded through the catalog (ref tip) | Yes, predates #1321 | `observe.rs:414-424` (`CoreRoleSource::Git` branch with `anywhere` workspace, `self.roles.load`); pre-#1321 version at `crates/jackin-tui/src/observe.rs` line 377 |
| Git-role picker launch test (ref tip) | Yes, updated | `crates/jackin-tui/tests/launch/picker.rs:135` `a_git_sourced_role_launches_from_the_picker`. Main's body is newer: it uses `ConfigEdit` with `CancellationToken`, and the expected agent is `AgentInstance::new(Codex, Zai, "work", "glm-5.3")` instead of the ref's `AgentInstance::lead(Codex, "work")`. This is a superseded expectation, not a loss. |

## Other changed content (section 3 classification)

- **Product code (`jackin-session`, `jackin-core`, `jackin-tui` outside the two files above):** the parent `4691c00896` snapshot differs from main. Its product content is covered by the sibling report's hunk tables, and it is held by kept checkpoints (see above). Deletion of 17/18 does not depend on this classification.
- **Ref-only files (43 A):**
  - `.github/*`, `.velnor/config.toml`, `.mise/locks/*`, `docs/*`, `rewrite/blockers/*`: excluded by plan section 3 (generated CI, docs, and rewrite material).
  - `crates/usage` fixtures (`gemini/*`, `opencode/auth*.json`, `kimi/usages_hours.json`, `kimi/usages_no_window.json`) and `providers/gemini.rs`: Gemini was removed by #1296 (excluded). The Kimi fixtures use an old `limits`/`window` schema; main's Kimi parser (`crates/usage/jackin-usage-provider-kimi/src/lib.rs:28-46`) reads the current `usages` schema. Superseded.
  - The sibling report did not re-read the `crates/usage` hunks line by line; this section is my spot check of the ref-only files only.
- **Modified/deleted usage crate files (M/D):** main is a redesigned layout (for example Gemini, MiMo and MiniMax provider crates removed or rewritten). Not restorable under plan section 3 (D16 and #1296 removals). Not checked line by line.

## Limits

- Presence checks are by file content, test name and reading of the code. No build, test run or CI run was done.
- The 469 M files were not compared line by line. Their product code is covered by the T-A argument in the sibling report.
- P08 test gaps found while checking (not a ref invariant, so not a VALUE for these refs). Track them under P08 closure:
  - No test for deleting, then restoring, a Path manifest on main. The code path exists (`observe.rs:371-383`, `Err` read leads to `Unreadable`, and a restored file reloads), but no test covers it.
  - No test that a Git-sourced role doc stays cached across refreshes (`observe.rs:392-393`).
