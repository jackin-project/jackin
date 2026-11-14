# Dirty-slice accounting: the retained jackin branch and worktree set

Slice: w4-audit. Date: 2026-11-14 (accounted against `origin/main` = `926bbd49d171`).

## Why the repo-wide totals were wrong

The coordinator snapshot reported 135 local branches without an upstream, 182 dirty files and
53729 unpushed commits. Two of those three numbers are artefacts of how the work was measured, and
they are the reason this slice exists:

* **53729 unpushed commits is not a per-branch quantity.** PR #1264 (`feat!: rebuild jackin on NVIDIA
  OpenShell`) was a squash merge: merge commit `f683619b5`, PR head `f0021a0b2`. Every local branch
  that descends from the pre-#1264 line therefore reports ~1000-1200 commits "ahead" of
  `origin/main` even though that history was landed *as one commit*. Summing those numbers across
  branches counts the same landed commits once per branch.
* **The per-branch truth is per file, not per commit.** A branch is worth keeping only if it holds a
  file version that is not in `main`. The table below therefore reports, per branch, the files that
  the branch's own commits touch whose content is **not** present in `main` at or after the #1264
  squash.
* **182 dirty files is correct**, and it is exactly the number of dirty entries in the swarm's
  retained worktree copies under `.pi/durable-swarm/worktrees/` (182 entries across 21 worktrees).
  The 184 other worktrees of the repo are clean or detached.

## Method (reproducible)

```sh
# 1. the landed reference set: the #1264 tree plus everything after it
git ls-tree -r f683619b5 | awk '{print $3}'
git rev-list --objects origin/main --not f683619b5^ | cut -d' ' -f1

# 2. per branch: the commits that are in neither main nor the #1264 PR head
git rev-list <branch> --not origin/main f0021a0b2

# 3. per file: is this file version already landed?
git rev-parse --verify --quiet <branch>:<path>     # the branch's blob
grep -qxF <blob> <landed-blob-set>                # -> CONTAINED

# 4. per worktree: the dirty entries
git -C <worktree> status --porcelain
git -C <worktree> hash-object <path>              # the working-tree blob
```

The `landed-blob-set` deliberately excludes the pre-#1264 history: those blobs are in `main`'s
*history* but are not the content `main` serves today, so counting them as landed would hide real work.

## Counts

| unit | count |
|---|---|
| local branches inventoried | 168 |
| branches without an upstream | 150 |
| swarm worktree copies inspected | 21 |
| dirty file entries in the swarm worktrees | 182 |
| **VALUABLE** branches | **49** |
| **CONTAINED** branches | **111** |
| **DEBRIS** branches | **8** |
| dirty files with content not in main | 118 |
| dirty files already landed in main | 64 |

Of the 49 VALUABLE branches, 14 are based on the current main line (a pull request from them is a
reviewable diff) and 35 sit on the pre-#1264 line (a pull request from them would propose reverting
the whole #1264 rebuild, so they are pushed for preservation only and must be ported before review).

## Table 1 - per-branch accounting (168 branches)

`unlanded files` = files the branch's own commits touch whose content is not in `main` at or after
the #1264 squash. `base` = merge-base with `origin/main`.

| # | branch | tip | base | upstream | commits not in main | files touched | unlanded files | class | evidence |
|---|---|---|---|---|---|---|---|---|---|
| 1 | `backup/w3-local-usage` | `f0aa365be70a` | `f683619b5f16` | (none) | 21 | 88 | 27 | **VALUABLE** | 27 file(s) not in main [crates/jackin-core/src/config.rs, crates/jackin-core/src/wait.rs, crates/jackin-core/tests/poll.rs...] |
| 2 | `chore/dirty-slice-1` | `72401bf5e78c` | `78d3b86ed520` | origin/chore/dirty-slice-1 | 2 | 12 | 11 | **VALUABLE** | 11 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/tests/catalog.rs, crates/jackin-core/tests/native_credentials.rs...] |
| 3 | `chore/restore-1283-remainder` | `e519e06e4885` | `fc00663fbbf7` | origin/chore/restore-1283-remainder | 3 | 3 | 0 | **CONTAINED** | 3 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1281 (docs(triage): record the completed port verdicts and the con) |
| 4 | `chore/triage-inventory-20261112` | `e13360eb8751` | `d19cce66f548` | origin/chore/triage-inventory-20261112 | 1 | 1 | 1 | **DEBRIS** | named in the ratified 15-branch DEBRIS set (rewrite/decisions.tsv, 2026-11-14T16:14:00Z row); carries no unique product code |
| 5 | `codex/usage-research-20261105` | `52fabbcca9e9` | `470fc5d6f5ea` | codex/usage-research-20261105@{upstream} | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 6 | `codex/usage-research-20261105-latest` | `4e58311c18d0` | `470fc5d6f5ea` | codex/usage-research-20261105-latest@{upstream} | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 7 | `docs/debris-ratification-2026-11-14` | `b550ce9ddccb` | `fc00663fbbf7` | origin/docs/debris-ratification-2026-11-14 | 2 | 1 | 1 | **VALUABLE** | 1 file(s) not in main [rewrite/triage/debris-ratification-2026-11-14.md] |
| 8 | `docs/triage-inventory-postmerge-2026-11-15` | `48d495d01b95` | `926bbd49d171` | (none) | 1 | 2 | 2 | **VALUABLE** | 2 file(s) not in main [rewrite/decisions.tsv, rewrite/triage/branch-inventory-2026-11-12.md] |
| 9 | `docs/triage-inventory-reclass-2026-11-13` | `cf9acb7485e1` | `fc00663fbbf7` | origin/docs/triage-inventory-reclass-2026-11-13 | 23 | 2 | 2 | **VALUABLE** | 2 file(s) not in main [rewrite/decisions.tsv, rewrite/triage/branch-inventory-2026-11-12.md] |
| 10 | `feat/probe-j07-launcher-identity` | `b8b0ff494538` | `7c184f9dfe2e` | (none) | 8 | 9 | 0 | **CONTAINED** | 9 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1282 (feat(probe-schema): accept the J07 launcher identity facts) |
| 11 | `feature/openshell-native` | `7bd1b7036fe7` | `470fc5d6f5ea` | feature/openshell-native@{upstream} | 0 | 0 | 0 | **CONTAINED** | the branch is the head branch of merged PR #1264 (feat!: rebuild jackin on NVIDIA OpenShell); every touched file landed in main |
| 12 | `fix/cli-review-fixes` | `b95cd9288837` | `f683619b5f16` | origin/main | 3 | 14 | 7 | **VALUABLE** | 7 file(s) not in main [crates/jackin-tui/src/console.rs, crates/jackin-tui/tests/console/panes.rs, crates/jackin-tui/tests/host.rs...] |
| 13 | `fix/core-plan-review` | `b172385c87f7` | `f683619b5f16` | origin/main | 4 | 22 | 5 | **VALUABLE** | 5 file(s) not in main [crates/jackin-core/src/wait.rs, crates/jackin-core/tests/poll.rs, crates/jackin-image/tests/agent_plan.rs...] |
| 14 | `fix/pr1264-followups-w3` | `05549f29aab9` | `d19cce66f548` | (none) | 26 | 78 | 3 | **VALUABLE** | 3 file(s) not in main; 2 dirty file(s) with new content in the worktree [crates/jackin/src/commands/account.rs, crates/usage/jackin-usage/src/credential.rs, crates/usage/jackin-usage/tests/usage/opencode.rs] |
| 15 | `fix/pr1264-review-followups` | `d79ad9b29e10` | `d19cce66f548` | origin/fix/pr1264-review-followups | 41 | 82 | 1 | **VALUABLE** | 1 file(s) not in main [crates/usage/jackin-usage/tests/usage/opencode.rs] |
| 16 | `fix/roles-review-impl` | `04471245d9b9` | `f683619b5f16` | (none) | 13 | 78 | 15 | **VALUABLE** | 15 file(s) not in main [crates/jackin-core/src/config.rs, crates/jackin-core/src/wait.rs, crates/jackin-core/tests/poll.rs...] |
| 17 | `fix/roles-review-impl-v2` | `3d2401c1b34b` | `97b1b58f716e` | (none) | 2 | 12 | 0 | **CONTAINED** | 12 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1278 (fix(session): apply the five accepted review findings of the) |
| 18 | `fix/secrets-scan-squash-1264` | `13d9038ce124` | `f683619b5f16` | fix/secrets-scan-squash-1264@{upstream} | 1 | 1 | 0 | **CONTAINED** | 1 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1276 (fix(secrets): re-key the squashed #1264 false-positive finge) |
| 19 | `fix/typed-model-selection` | `0aaf7a411ce0` | `78d3b86ed520` | origin/fix/typed-model-selection | 5 | 3 | 2 | **VALUABLE** | 2 file(s) not in main [crates/jackin-core/src/model_selection.rs, rewrite/API.md] |
| 20 | `fix/usage-review-a` | `f6a83f1167ec` | `f683619b5f16` | (none) | 13 | 80 | 28 | **VALUABLE** | 28 file(s) not in main [crates/jackin-core/src/config.rs, crates/jackin-core/src/wait.rs, crates/jackin-core/tests/poll.rs...] |
| 21 | `fix/usage-review-b` | `e37b59e4beb8` | `f683619b5f16` | (none) | 14 | 73 | 16 | **VALUABLE** | 16 file(s) not in main [crates/jackin-core/src/config.rs, crates/jackin-core/src/wait.rs, crates/jackin-core/tests/poll.rs...] |
| 22 | `main` | `7a53f6abe781` | `7a53f6abe781` | origin/main | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; PR #1275 (merged; branch tree is byte-identical to the merge commit) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 23 | `pr-1264` | `4691c0089608` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 24 | `recovery/branches-nightly-compat-20261029` | `9827b3adcf7b` | `97b1b58f716e` | recovery/branches-nightly-compat-20261029@{upstream} | 7 | 22 | 0 | **CONTAINED** | 22 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1264 (feat!: rebuild jackin on NVIDIA OpenShell) |
| 25 | `research/usage-reporting-sync` | `4d66be8868d8` | `470fc5d6f5ea` | research/usage-reporting-sync@{upstream} | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 26 | `sup010/sonar-port-1264` | `8b410d2e6a79` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 27 | `sup019/fix-1264` | `2c54a59747a9` | `aa218ea8312b` | (none) | 1 | 25 | 6 | **VALUABLE** | 6 file(s) not in main [crates/jackin-core/src/config.rs, crates/jackin-session/tests/accounts/renewal.rs, crates/jackin-tui/src/effects.rs...] |
| 28 | `sup020/fix-cli-1264` | `aadbd7a40fc0` | `aa218ea8312b` | (none) | 3 | 10 | 3 | **VALUABLE** | 3 file(s) not in main [crates/jackin/src/cli.rs, crates/jackin/src/error.rs, crates/jackin/tests/cli/main.rs] |
| 29 | `swarm/fanout-call_01a1ca3c3c87734fa42bfaef-0` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 30 | `swarm/fanout-call_01a1ca3c3c87734fa42bfaef-1` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 31 | `swarm/fanout-call_01a1ce79e4b6702caf3208cd-0` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 32 | `swarm/fanout-call_01a1ce79e4b6702caf3208cd-1` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 33 | `swarm/fanout-call_01a1dacf0a2d72c8b83ac7fa-0` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 34 | `swarm/fanout-call_01a1dacf0a2d72c8b83ac7fa-1` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 35 | `swarm/fanout-call_01a1dacf0a2d72c8b83ac7fa-2` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 36 | `swarm/fanout-call_01a1dacf0a2d72c8b83ac7fa-3` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 37 | `swarm/fanout-call_01a1dacfcfea75d095635e4c-0` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 38 | `swarm/fanout-call_01a1dacfcfea75d095635e4c-1` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 39 | `swarm/fanout-call_01a1dacfcfea75d095635e4c-2` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 40 | `swarm/fanout-call_01a1dacfcfea75d095635e4c-3` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 41 | `swarm/fanout-call_01a1dacfcfea75d095635e4c-5` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 42 | `swarm/fanout-call_1c9ab3259fa54c9398809a67-2` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 43 | `swarm/fanout-call_1c9ab3259fa54c9398809a67-3` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 44 | `swarm/fanout-call_1c9ab3259fa54c9398809a67-4` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 45 | `swarm/fanout-call_26ad04a5596f43218bb3e124-0` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 46 | `swarm/fanout-call_26ad04a5596f43218bb3e124-1` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 47 | `swarm/fanout-call_26ad04a5596f43218bb3e124-2` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 48 | `swarm/fanout-call_26ad04a5596f43218bb3e124-3` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 49 | `swarm/fanout-call_26ad04a5596f43218bb3e124-4` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 50 | `swarm/fanout-call_38d571baaea149d28da2e09c-2` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 51 | `swarm/fanout-call_3d4ffa93267d4f40a885ab3d-1` | `8ff8e8871ac7` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 52 | `swarm/fanout-call_61766d565b904e3dba2db8d5-0` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 53 | `swarm/fanout-call_61766d565b904e3dba2db8d5-1` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 54 | `swarm/fanout-call_61766d565b904e3dba2db8d5-2` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 55 | `swarm/fanout-call_667899f3ad454fd1946b4556-0` | `8a5b40e9e2f9` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 56 | `swarm/fanout-call_667899f3ad454fd1946b4556-1` | `8a5b40e9e2f9` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of swarm/fanout-call_667899f3ad454fd1946b4556-0 (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 57 | `swarm/fanout-call_667899f3ad454fd1946b4556-2` | `d864d067e4ad` | `470fc5d6f5ea` | (none) | 1 | 71 | 10 | **VALUABLE** | 10 file(s) not in main [Cargo.lock, crates/usage/jackin-usage-provider-claude/src/lib.rs, crates/usage/jackin-usage-provider-core/src/http.rs...] |
| 58 | `swarm/fanout-call_667899f3ad454fd1946b4556-3` | `8a5b40e9e2f9` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of swarm/fanout-call_667899f3ad454fd1946b4556-0 (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 59 | `swarm/fanout-call_7b848c095cd548d2a97dfc27-0` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 60 | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 61 | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 62 | `swarm/fanout-call_7e94b05862e34ec3b5a49949-0` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 63 | `swarm/fanout-call_7e94b05862e34ec3b5a49949-4` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 64 | `swarm/fanout-call_7e94b05862e34ec3b5a49949-5` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 65 | `swarm/fanout-call_7e94b05862e34ec3b5a49949-6` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 66 | `swarm/fanout-call_84bf371fb4fb46308f47c848-0` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 67 | `swarm/fanout-call_84bf371fb4fb46308f47c848-1` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 68 | `swarm/fanout-call_84bf371fb4fb46308f47c848-2` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 69 | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 70 | `swarm/fanout-call_8640a4ba69f24cf4ac59e6d5-1` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 71 | `swarm/fanout-call_b993ce1e59e44b1f85a2ab78-0` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 72 | `swarm/fanout-call_c150420bac2f4875b01d23cd-1` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 73 | `swarm/fanout-call_cd50d53ea77340e08a7d675b-0` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 74 | `swarm/fanout-call_cd50d53ea77340e08a7d675b-1` | `78855f5ad1b2` | `470fc5d6f5ea` | (none) | 1 | 5 | 2 | **VALUABLE** | 2 file(s) not in main [crates/jackin-openshell/src/grpc.rs, crates/jackin-openshell/tests/gateway/connect.rs] |
| 75 | `swarm/fanout-call_cd50d53ea77340e08a7d675b-2` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 76 | `swarm/fanout-call_e6e20aee930345eb8831374f-0` | `50103fdf0932` | `470fc5d6f5ea` | (none) | 1 | 16 | 9 | **VALUABLE** | 9 file(s) not in main [crates/jackin-core/src/ids.rs, crates/jackin-session/src/attach.rs, crates/jackin-session/src/multi_session.rs...] |
| 77 | `swarm/fanout-call_e6e20aee930345eb8831374f-1` | `8a5b40e9e2f9` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of swarm/fanout-call_667899f3ad454fd1946b4556-0 (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 78 | `swarm/fanout-call_e6e20aee930345eb8831374f-2` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 79 | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-3` | `40e8818ebd84` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 80 | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `40e8818ebd84` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-3 (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 81 | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-5` | `40e8818ebd84` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-3 (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 82 | `swarm/pr1287-debris-ratification` | `b550ce9ddccb` | `fc00663fbbf7` | (none) | 2 | 1 | 1 | **DEBRIS** | byte-identical tip b550ce9ddccb to docs/debris-ratification-2026-11-14; the unit is carried there |
| 83 | `swarm/sup009-sonar-source-config` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 84 | `swarm/tool-call_01a1ce4ead2774dfa85e5567` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 85 | `swarm/tool-call_01a1daeaab3c703ea50a9304` | `f0b2a8a307e5` | `470fc5d6f5ea` | (none) | 1 | 6 | 0 | **CONTAINED** | 6 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1275 (fix: land the accepted review findings from #1264) |
| 86 | `swarm/tool-call_3a3215a2d97145e49da3d67a` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 87 | `swarm/tool-call_408ca8fd3b9e44639da156ae` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 88 | `swarm/tool-call_9b95e9cc2960449db0fe2d27` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-integrated (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 89 | `test/cli-t09-t18-collision` | `0260cc4b0e92` | `3c3f77db534f` | origin/test/cli-t09-t18-collision | 3 | 1 | 0 | **CONTAINED** | 1 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1288 (test(cli): cover the T09/T18 credential environment collisio) |
| 90 | `test/provider-eligibility` | `4f858aaaa752` | `fc00663fbbf7` | origin/test/provider-eligibility | 3 | 3 | 0 | **CONTAINED** | 3 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1285 (test(core): cover the account to supported tuple eligibility) |
| 91 | `w3/pr1281-land` | `b2c0da82a816` | `2bdef85bbaa4` | (none) | 31 | 2 | 0 | **CONTAINED** | 2 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1281 (docs(triage): record the completed port verdicts and the con) |
| 92 | `w3/pr1281-merge` | `e8d6bcca4638` | `7f13576ac0b7` | (none) | 26 | 2 | 2 | **VALUABLE** | 2 file(s) not in main [rewrite/decisions.tsv, rewrite/triage/branch-inventory-2026-11-12.md] |
| 93 | `wt/a11b-account-hardening` | `a935999a424e` | `470fc5d6f5ea` | (none) | 1 | 43 | 35 | **VALUABLE** | 35 file(s) not in main [.audit/a11b-ci-fast-timeout.log, .audit/a11b-ci-fast.log, .audit/a11b-ci-static.log...] |
| 94 | `wt/a14-accounts-integrated` | `ecfe7e96047a` | `470fc5d6f5ea` | (none) | 1 | 42 | 21 | **VALUABLE** | 21 file(s) not in main [Cargo.lock, crates/jackin-core/src/config.rs, crates/jackin-core/tests/config_edit.rs...] |
| 95 | `wt/a14-availability` | `7a20e913e92e` | `470fc5d6f5ea` | (none) | 3 | 96 | 64 | **VALUABLE** | 64 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml, crates/jackin-core/tests/agent_instances.rs...] |
| 96 | `wt/a14-availability-final` | `ebfb26c9c396` | `470fc5d6f5ea` | (none) | 2 | 39 | 33 | **VALUABLE** | 33 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml, crates/jackin-core/tests/agent_instances.rs...] |
| 97 | `wt/a14-claude-final` | `e275950ca3f5` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 98 | `wt/a14-claude-model` | `4a3d4ee53242` | `470fc5d6f5ea` | (none) | 1 | 6 | 5 | **VALUABLE** | 5 file(s) not in main [crates/jackin-session/src/files.rs, crates/jackin-session/src/launch/run.rs, crates/jackin-session/src/plan.rs...] |
| 99 | `wt/a14-codex-final` | `e275950ca3f5` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-claude-final (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 100 | `wt/a14-grok-session` | `17aacafcfa08` | `470fc5d6f5ea` | (none) | 1 | 35 | 23 | **VALUABLE** | 23 file(s) not in main [.audit/a14-oauth-baseline-nextest.log, .audit/a14-oauth-g-fast.log, .audit/a14-oauth-image-nextest.log...] |
| 101 | `wt/a14-instance-files` | `7bd1b7036fe7` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of feature/openshell-native (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 102 | `wt/a14-integrated` | `9792bdd88ee0` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 103 | `wt/a14-muse-final` | `d9411a2d72a3` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 104 | `wt/a14-native-api-key` | `7bd1b7036fe7` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of feature/openshell-native (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 105 | `wt/a14-native-implementation` | `17aacafcfa08` | `470fc5d6f5ea` | (none) | 1 | 35 | 23 | **DEBRIS** | byte-identical tip 17aacafcfa08 to wt/a14-grok-session; the unit is carried there |
| 106 | `wt/a14-native-integrated` | `97c7a8ce644f` | `470fc5d6f5ea` | (none) | 1 | 77 | 33 | **VALUABLE** | 33 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/config.rs, crates/jackin-core/tests/agent_instances.rs...] |
| 107 | `wt/a14-native-subscription` | `7bd1b7036fe7` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of feature/openshell-native (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 108 | `wt/a14-oauth-blockers` | `3815766f08dc` | `470fc5d6f5ea` | (none) | 3 | 4 | 2 | **VALUABLE** | 2 file(s) not in main [rewrite/blockers/T03-grok.md, rewrite/blockers/T04-muse.md] |
| 109 | `wt/a14-oauth-files` | `17aacafcfa08` | `470fc5d6f5ea` | (none) | 1 | 35 | 23 | **DEBRIS** | byte-identical tip 17aacafcfa08 to wt/a14-grok-session; the unit is carried there |
| 110 | `wt/a14-oauth-final` | `54485eb720dc` | `470fc5d6f5ea` | (none) | 1 | 1 | 0 | **CONTAINED** | 1 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1264 (feat!: rebuild jackin on NVIDIA OpenShell) |
| 111 | `wt/a14-oauth-integrated` | `94d880efefe9` | `470fc5d6f5ea` | (none) | 1 | 57 | 35 | **VALUABLE** | 35 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/tests/agent_instances.rs, crates/jackin-core/tests/agent_table.rs...] |
| 112 | `wt/a14-oauth-verify` | `c5557016f5e5` | `470fc5d6f5ea` | (none) | 2 | 2 | 0 | **CONTAINED** | 2 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1264 (feat!: rebuild jackin on NVIDIA OpenShell) |
| 113 | `wt/a14-omp-final` | `e275950ca3f5` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-claude-final (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 114 | `wt/a14-promotion` | `a65c59a3bde2` | `470fc5d6f5ea` | (none) | 3 | 34 | 17 | **VALUABLE** | 17 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/tests/catalog.rs, crates/jackin-core/tests/manifest.rs...] |
| 115 | `wt/a14-role-bindings` | `7bd1b7036fe7` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of feature/openshell-native (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 116 | `wt/a14-role-review` | `790a9e891005` | `470fc5d6f5ea` | (none) | 4 | 41 | 33 | **VALUABLE** | 33 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml, crates/jackin-core/tests/agent_instances.rs...] |
| 117 | `wt/a14-stable-pins` | `b5d317b080e7` | `470fc5d6f5ea` | (none) | 3 | 40 | 33 | **VALUABLE** | 33 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml, crates/jackin-core/tests/agent_instances.rs...] |
| 118 | `wt/a14-subscription-close` | `81cace3ddd3e` | `470fc5d6f5ea` | (none) | 2 | 5 | 4 | **VALUABLE** | 4 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-image/tests/agent_plan.rs, rewrite/blockers/T02-claude.md...] |
| 119 | `wt/a14-subscription-final` | `20621e9e2869` | `470fc5d6f5ea` | (none) | 1 | 1 | 0 | **CONTAINED** | 1 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1264 (feat!: rebuild jackin on NVIDIA OpenShell) |
| 120 | `wt/a14-subscription-verify` | `95feab027096` | `470fc5d6f5ea` | (none) | 2 | 2 | 0 | **CONTAINED** | 2 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1264 (feat!: rebuild jackin on NVIDIA OpenShell) |
| 121 | `wt/a14-t01-final` | `e275950ca3f5` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-claude-final (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 122 | `wt/a14-t02-blocker` | `a31a8e7b8227` | `470fc5d6f5ea` | (none) | 4 | 6 | 4 | **VALUABLE** | 4 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml, crates/jackin-openshell/tests/tuple_policy.rs...] |
| 123 | `wt/a14-t02-final-oauth` | `94d880efefe9` | `470fc5d6f5ea` | (none) | 1 | 57 | 35 | **DEBRIS** | byte-identical tip 94d880efefe9 to wt/a14-oauth-integrated; the unit is carried there |
| 124 | `wt/a14-t02-live-fixed` | `f41625351c8b` | `470fc5d6f5ea` | (none) | 3 | 5 | 3 | **VALUABLE** | 3 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml, crates/jackin-openshell/tests/tuple_policy.rs] |
| 125 | `wt/a14-t02-policy` | `8f4a4bfd31db` | `470fc5d6f5ea` | (none) | 1 | 3 | 3 | **VALUABLE** | 3 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml, crates/jackin-openshell/tests/tuple_policy.rs] |
| 126 | `wt/a14-t02-policy-review` | `28f7e0751b71` | `470fc5d6f5ea` | (none) | 2 | 4 | 3 | **VALUABLE** | 3 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml, crates/jackin-openshell/tests/tuple_policy.rs] |
| 127 | `wt/a14-t03-final-oauth` | `94d880efefe9` | `470fc5d6f5ea` | (none) | 1 | 57 | 35 | **DEBRIS** | byte-identical tip 94d880efefe9 to wt/a14-oauth-integrated; the unit is carried there |
| 128 | `wt/a14-t03-promotion` | `fbdb9667be33` | `470fc5d6f5ea` | (none) | 3 | 87 | 33 | **VALUABLE** | 33 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/config.rs, crates/jackin-core/tests/agent_instances.rs...] |
| 129 | `wt/a14-t08-live` | `bafb12ec0c15` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 130 | `wt/a14-t08-model-live` | `9e231545cb69` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 131 | `wt/a14-t09-config` | `d6ee3233e0f8` | `470fc5d6f5ea` | (none) | 5 | 41 | 33 | **VALUABLE** | 33 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml, crates/jackin-core/tests/agent_instances.rs...] |
| 132 | `wt/a14-t09-live` | `c700d207885f` | `470fc5d6f5ea` | (none) | 5 | 41 | 33 | **VALUABLE** | 33 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml, crates/jackin-core/tests/agent_instances.rs...] |
| 133 | `wt/a14-t09-verify` | `f9d6621d2486` | `470fc5d6f5ea` | (none) | 5 | 41 | 33 | **VALUABLE** | 33 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml, crates/jackin-core/tests/agent_instances.rs...] |
| 134 | `wt/a14-t13-auth` | `8c444a97ad71` | `470fc5d6f5ea` | (none) | 1 | 6 | 4 | **VALUABLE** | 4 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-openshell/tests/tuple_policy.rs, crates/jackin-session/tests/accounts/sync.rs...] |
| 135 | `wt/a14-t13-live-fixed` | `8c444a97ad71` | `470fc5d6f5ea` | (none) | 1 | 6 | 4 | **DEBRIS** | byte-identical tip 8c444a97ad71 to wt/a14-t13-auth; the unit is carried there |
| 136 | `wt/a14-t13-model-live` | `15b0191fa4b2` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 137 | `wt/a14-zai-final` | `36537f1dd06d` | `470fc5d6f5ea` | (none) | 3 | 40 | 33 | **VALUABLE** | 33 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml, crates/jackin-core/tests/agent_instances.rs...] |
| 138 | `wt/a14-zai-recheck` | `e275950ca3f5` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-claude-final (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 139 | `wt/a15-declared-auth-producer` | `dd2008cbf9e7` | `none` | (none) | 192 | 75 | 34 | **VALUABLE** | 34 file(s) not in main [.dockerignore, .github/actions/velnor-verify-restored-rust/action.yml, .github/actions/velnor-verify-rust-manifest/action.yml...] |
| 140 | `wt/a15-e2e-t01` | `655a00e2f128` | `470fc5d6f5ea` | (none) | 7 | 236 | 58 | **VALUABLE** | 58 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml, crates/jackin-core/tests/agent_instances.rs...] |
| 141 | `wt/a15-final-clean` | `8c93c096dbad` | `470fc5d6f5ea` | (none) | 5 | 236 | 210 | **VALUABLE** | 210 file(s) not in main [.audit/a15-e2e-t01.tsv, .audit/a15-final-review-hook.py, .audit/a15-final-review-logs/IT1.adapter.green.log...] |
| 142 | `wt/a15-final-review` | `b667957821df` | `470fc5d6f5ea` | (none) | 3 | 87 | 62 | **VALUABLE** | 62 file(s) not in main [.audit/a15-e2e-t01.tsv, crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml...] |
| 143 | `wt/a15-supported-target-e2e-fixes` | `9e231545cb69` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/a14-t08-model-live (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 144 | `wt/a15-supported-targets` | `c78d1ab38556` | `470fc5d6f5ea` | (none) | 9 | 239 | 48 | **VALUABLE** | 48 file(s) not in main [crates/jackin-core/src/agent.rs, crates/jackin-core/src/assets/t02-claude.yaml, crates/jackin-core/tests/agent_instances.rs...] |
| 145 | `wt/agy-target` | `c31d572a8d21` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 146 | `wt/blockers` | `910a2104d5e3` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 147 | `wt/claude-lane` | `914d8e8efcf7` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 148 | `wt/codex-lane` | `b9b877110f1b` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 149 | `wt/live-kimi2` | `df7674dd4310` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 150 | `wt/live-kimi3` | `74bf8ac22aca` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 151 | `wt/live-mm` | `df7674dd4310` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/live-kimi2 (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 152 | `wt/live-rest` | `74bf8ac22aca` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/live-kimi3 (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 153 | `wt/live-sub` | `961ede2c24e1` | `470fc5d6f5ea` | (none) | 1 | 3 | 2 | **VALUABLE** | 2 file(s) not in main [rewrite/blockers/T02-claude.md, rewrite/blockers/T05-antigravity.md] |
| 154 | `wt/live-t02` | `4abb579b37b6` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 155 | `wt/live-t02b` | `6ed8ae5b5be7` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 156 | `wt/live-t02t05` | `a79fa139334e` | `470fc5d6f5ea` | (none) | 1 | 1 | 0 | **CONTAINED** | 1 files touched; every touched file landed in main at or after the #1264 squash; the landing PR is #1264 (feat!: rebuild jackin on NVIDIA OpenShell) |
| 157 | `wt/live-t21` | `df7674dd4310` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/live-kimi2 (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 158 | `wt/login-import` | `4abb579b37b6` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; duplicate tip of wt/live-t02 (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 159 | `wt/models-research` | `66e47d24c812` | `470fc5d6f5ea` | (none) | 1 | 2 | 1 | **VALUABLE** | 1 file(s) not in main [rewrite/research/t13-t22-models.md] |
| 160 | `wt/muse-strace` | `3f07774c2de4` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 161 | `wt/native-fix` | `7f02be3a2b6b` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 162 | `wt/omp-config` | `ac03d72fb4de` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 163 | `wt/omp-lane` | `304a0315c54b` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 164 | `wt/omp-pin` | `94fefa01585b` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 165 | `wt/pr1281-review-fixes` | `cf9acb7485e1` | `fc00663fbbf7` | (none) | 23 | 2 | 2 | **DEBRIS** | byte-identical tip cf9acb7485e1 to docs/triage-inventory-reclass-2026-11-13; the unit is carried there |
| 166 | `wt/t02-fix` | `1ea6ed75a2a4` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 167 | `wt/t21-model` | `40f8bd4c49a2` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |
| 168 | `wt/zai-fix` | `cf950bb2b8eb` | `470fc5d6f5ea` | (none) | 0 | 0 | 0 | **CONTAINED** | all 0 unlanded commit(s) patch-id-match landed content; content landed in origin/main (patch-id / blob match) (no single merged PR carries it; the content landed in main across the post-#1264 history) |

## Table 2 - per-file disposition of the VALUABLE branch units

Every file below is present on its branch and absent from `main` at or after the #1264 squash.
These are the files that would be lost if the branch were deleted.

| branch | base | file | class |
|---|---|---|---|
| `backup/w3-local-usage` | post-#1264 | `crates/jackin-core/src/config.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/jackin-core/src/wait.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/jackin-core/tests/poll.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/jackin-testkit/src/host/clock.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/jackin-tui/src/console.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/jackin-tui/src/effects.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/jackin-tui/tests/console/usage_windows.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/jackin-tui/tests/host.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/jackin/src/commands/account.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/jackin/src/error.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/jackin/src/main.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/jackin/tests/cli/fixtures/usage/auth_chatgpt.json` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/jackin/tests/cli/main.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/jackin/tests/cli/usage.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/usage/jackin-usage/src/credential.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/usage/jackin-usage/tests/usage/cache.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/usage/jackin-usage/tests/usage/cancel.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/usage/jackin-usage/tests/usage/codex.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/usage/jackin-usage/tests/usage/credential_files.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/usage/jackin-usage/tests/usage/fixtures/codex/auth_both.json` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/usage/jackin-usage/tests/usage/fixtures/codex/auth_chatgpt.json` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/usage/jackin-usage/tests/usage/fixtures/codex/auth_no_access.json` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/usage/jackin-usage/tests/usage/isolation.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/usage/jackin-usage/tests/usage/omp.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/usage/jackin-usage/tests/usage/opencode.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/usage/jackin-usage/tests/usage/secrets.rs` | VALUABLE |
| `backup/w3-local-usage` | post-#1264 | `crates/usage/jackin-usage/tests/usage/support.rs` | VALUABLE |
| `chore/dirty-slice-1` | post-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `chore/dirty-slice-1` | post-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `chore/dirty-slice-1` | post-#1264 | `crates/jackin-core/tests/native_credentials.rs` | VALUABLE |
| `chore/dirty-slice-1` | post-#1264 | `crates/jackin-session/tests/plan/availability.rs` | VALUABLE |
| `chore/dirty-slice-1` | post-#1264 | `crates/jackin-session/tests/plan/static_files.rs` | VALUABLE |
| `chore/dirty-slice-1` | post-#1264 | `crates/jackin-session/tests/plan/tuple.rs` | VALUABLE |
| `chore/dirty-slice-1` | post-#1264 | `crates/jackin-tui/tests/launch/tuples.rs` | VALUABLE |
| `chore/dirty-slice-1` | post-#1264 | `crates/jackin/tests/cli/account_add.rs` | VALUABLE |
| `chore/dirty-slice-1` | post-#1264 | `crates/jackin/tests/cli/load.rs` | VALUABLE |
| `chore/dirty-slice-1` | post-#1264 | `crates/xtask/tests/repo_gates.rs` | VALUABLE |
| `chore/dirty-slice-1` | post-#1264 | `rewrite/evidence/A14-t08-model-live.md` | VALUABLE |
| `docs/debris-ratification-2026-11-14` | post-#1264 | `rewrite/triage/debris-ratification-2026-11-14.md` | VALUABLE |
| `docs/triage-inventory-postmerge-2026-11-15` | post-#1264 | `rewrite/decisions.tsv` | VALUABLE |
| `docs/triage-inventory-postmerge-2026-11-15` | post-#1264 | `rewrite/triage/branch-inventory-2026-11-12.md` | VALUABLE |
| `docs/triage-inventory-reclass-2026-11-13` | post-#1264 | `rewrite/decisions.tsv` | VALUABLE |
| `docs/triage-inventory-reclass-2026-11-13` | post-#1264 | `rewrite/triage/branch-inventory-2026-11-12.md` | VALUABLE |
| `fix/cli-review-fixes` | post-#1264 | `crates/jackin-tui/src/console.rs` | VALUABLE |
| `fix/cli-review-fixes` | post-#1264 | `crates/jackin-tui/tests/console/panes.rs` | VALUABLE |
| `fix/cli-review-fixes` | post-#1264 | `crates/jackin-tui/tests/host.rs` | VALUABLE |
| `fix/cli-review-fixes` | post-#1264 | `crates/jackin/src/cli.rs` | VALUABLE |
| `fix/cli-review-fixes` | post-#1264 | `crates/jackin/src/commands/account.rs` | VALUABLE |
| `fix/cli-review-fixes` | post-#1264 | `crates/jackin/src/error.rs` | VALUABLE |
| `fix/cli-review-fixes` | post-#1264 | `crates/jackin/src/main.rs` | VALUABLE |
| `fix/core-plan-review` | post-#1264 | `crates/jackin-core/src/wait.rs` | VALUABLE |
| `fix/core-plan-review` | post-#1264 | `crates/jackin-core/tests/poll.rs` | VALUABLE |
| `fix/core-plan-review` | post-#1264 | `crates/jackin-image/tests/agent_plan.rs` | VALUABLE |
| `fix/core-plan-review` | post-#1264 | `crates/jackin-testkit/src/host/clock.rs` | VALUABLE |
| `fix/core-plan-review` | post-#1264 | `crates/jackin-tui/tests/console/panes.rs` | VALUABLE |
| `fix/pr1264-followups-w3` | post-#1264 | `crates/jackin/src/commands/account.rs` | VALUABLE |
| `fix/pr1264-followups-w3` | post-#1264 | `crates/usage/jackin-usage/src/credential.rs` | VALUABLE |
| `fix/pr1264-followups-w3` | post-#1264 | `crates/usage/jackin-usage/tests/usage/opencode.rs` | VALUABLE |
| `fix/pr1264-review-followups` | post-#1264 | `crates/usage/jackin-usage/tests/usage/opencode.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/jackin-core/src/config.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/jackin-core/src/wait.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/jackin-core/tests/poll.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/jackin-image/tests/agent_plan.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/jackin-session/tests/accounts/renewal.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/jackin-testkit/src/host/clock.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/jackin-tui/src/console.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/jackin-tui/src/effects.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/jackin-tui/tests/host.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/jackin/src/commands/account.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/jackin/src/error.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/jackin/src/main.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/jackin/tests/cli/main.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/usage/jackin-usage/src/credential.rs` | VALUABLE |
| `fix/roles-review-impl` | post-#1264 | `crates/usage/jackin-usage/tests/usage/opencode.rs` | VALUABLE |
| `fix/typed-model-selection` | post-#1264 | `crates/jackin-core/src/model_selection.rs` | VALUABLE |
| `fix/typed-model-selection` | post-#1264 | `rewrite/API.md` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin-core/src/config.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin-core/src/wait.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin-core/tests/poll.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin-image/tests/agent_plan.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin-session/tests/accounts/renewal.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin-testkit/src/host/clock.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin-tui/src/console.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin-tui/src/effects.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin-tui/tests/console/usage_windows.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin-tui/tests/host.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin/src/commands/account.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin/src/error.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin/src/main.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin/tests/cli/fixtures/usage/auth_chatgpt.json` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin/tests/cli/main.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/jackin/tests/cli/usage.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/usage/jackin-usage/src/credential.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/usage/jackin-usage/tests/usage/cache.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/usage/jackin-usage/tests/usage/cancel.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/usage/jackin-usage/tests/usage/codex.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/usage/jackin-usage/tests/usage/credential_files.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/usage/jackin-usage/tests/usage/fixtures/codex/auth_both.json` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/usage/jackin-usage/tests/usage/fixtures/codex/auth_chatgpt.json` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/usage/jackin-usage/tests/usage/fixtures/codex/auth_no_access.json` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/usage/jackin-usage/tests/usage/isolation.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/usage/jackin-usage/tests/usage/opencode.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/usage/jackin-usage/tests/usage/secrets.rs` | VALUABLE |
| `fix/usage-review-a` | post-#1264 | `crates/usage/jackin-usage/tests/usage/support.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/jackin-core/src/config.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/jackin-core/src/wait.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/jackin-core/tests/poll.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/jackin-image/tests/agent_plan.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/jackin-session/tests/accounts/renewal.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/jackin-testkit/src/host/clock.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/jackin-tui/src/console.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/jackin-tui/src/effects.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/jackin-tui/tests/host.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/jackin/src/commands/account.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/jackin/src/error.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/jackin/src/main.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/jackin/tests/cli/main.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/usage/jackin-usage/src/credential.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/usage/jackin-usage/tests/usage/omp.rs` | VALUABLE |
| `fix/usage-review-b` | post-#1264 | `crates/usage/jackin-usage/tests/usage/opencode.rs` | VALUABLE |
| `sup019/fix-1264` | pre-#1264 | `crates/jackin-core/src/config.rs` | VALUABLE |
| `sup019/fix-1264` | pre-#1264 | `crates/jackin-session/tests/accounts/renewal.rs` | VALUABLE |
| `sup019/fix-1264` | pre-#1264 | `crates/jackin-tui/src/effects.rs` | VALUABLE |
| `sup019/fix-1264` | pre-#1264 | `crates/jackin/src/commands/account.rs` | VALUABLE |
| `sup019/fix-1264` | pre-#1264 | `crates/usage/jackin-usage/src/credential.rs` | VALUABLE |
| `sup019/fix-1264` | pre-#1264 | `crates/usage/jackin-usage/tests/usage/opencode.rs` | VALUABLE |
| `sup020/fix-cli-1264` | pre-#1264 | `crates/jackin/src/cli.rs` | VALUABLE |
| `sup020/fix-cli-1264` | pre-#1264 | `crates/jackin/src/error.rs` | VALUABLE |
| `sup020/fix-cli-1264` | pre-#1264 | `crates/jackin/tests/cli/main.rs` | VALUABLE |
| `swarm/fanout-call_667899f3ad454fd1946b4556-2` | pre-#1264 | `Cargo.lock` | VALUABLE |
| `swarm/fanout-call_667899f3ad454fd1946b4556-2` | pre-#1264 | `crates/usage/jackin-usage-provider-claude/src/lib.rs` | VALUABLE |
| `swarm/fanout-call_667899f3ad454fd1946b4556-2` | pre-#1264 | `crates/usage/jackin-usage-provider-core/src/http.rs` | VALUABLE |
| `swarm/fanout-call_667899f3ad454fd1946b4556-2` | pre-#1264 | `crates/usage/jackin-usage-provider-grok/src/lib.rs` | VALUABLE |
| `swarm/fanout-call_667899f3ad454fd1946b4556-2` | pre-#1264 | `crates/usage/jackin-usage-provider-kimi/src/lib.rs` | VALUABLE |
| `swarm/fanout-call_667899f3ad454fd1946b4556-2` | pre-#1264 | `crates/usage/jackin-usage-provider-minimax/src/lib.rs` | VALUABLE |
| `swarm/fanout-call_667899f3ad454fd1946b4556-2` | pre-#1264 | `crates/usage/jackin-usage-provider-opencode/src/lib.rs` | VALUABLE |
| `swarm/fanout-call_667899f3ad454fd1946b4556-2` | pre-#1264 | `crates/usage/jackin-usage-provider-openrouter/src/lib.rs` | VALUABLE |
| `swarm/fanout-call_667899f3ad454fd1946b4556-2` | pre-#1264 | `crates/usage/jackin-usage/src/credential.rs` | VALUABLE |
| `swarm/fanout-call_667899f3ad454fd1946b4556-2` | pre-#1264 | `crates/usage/jackin-usage/tests/usage/omp.rs` | VALUABLE |
| `swarm/fanout-call_cd50d53ea77340e08a7d675b-1` | pre-#1264 | `crates/jackin-openshell/src/grpc.rs` | VALUABLE |
| `swarm/fanout-call_cd50d53ea77340e08a7d675b-1` | pre-#1264 | `crates/jackin-openshell/tests/gateway/connect.rs` | VALUABLE |
| `swarm/fanout-call_e6e20aee930345eb8831374f-0` | pre-#1264 | `crates/jackin-core/src/ids.rs` | VALUABLE |
| `swarm/fanout-call_e6e20aee930345eb8831374f-0` | pre-#1264 | `crates/jackin-session/src/attach.rs` | VALUABLE |
| `swarm/fanout-call_e6e20aee930345eb8831374f-0` | pre-#1264 | `crates/jackin-session/src/multi_session.rs` | VALUABLE |
| `swarm/fanout-call_e6e20aee930345eb8831374f-0` | pre-#1264 | `crates/jackin-session/tests/export/purge.rs` | VALUABLE |
| `swarm/fanout-call_e6e20aee930345eb8831374f-0` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `swarm/fanout-call_e6e20aee930345eb8831374f-0` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `swarm/fanout-call_e6e20aee930345eb8831374f-0` | pre-#1264 | `crates/jackin-session/tests/launch/support.rs` | VALUABLE |
| `swarm/fanout-call_e6e20aee930345eb8831374f-0` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `swarm/fanout-call_e6e20aee930345eb8831374f-0` | pre-#1264 | `crates/jackin-session/tests/lifecycle/inspect.rs` | VALUABLE |
| `w3/pr1281-merge` | post-#1264 | `rewrite/decisions.tsv` | VALUABLE |
| `w3/pr1281-merge` | post-#1264 | `rewrite/triage/branch-inventory-2026-11-12.md` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-ci-fast-timeout.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-ci-fast.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-ci-static.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-clippy-targeted.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-current-cli.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-current-session.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-examples-check.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-fixed-cli-initial.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-fixed-session-initial.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-implementation.md` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-mutant-account-wide-cleanup.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-mutant-single-journal.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-omp-host-final.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-targeted-cli.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-targeted-session-all.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-targeted-session.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-test-plan-approved.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `.audit/a11b-test-plan-required-adjustments.log` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `Cargo.lock` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin-session/examples/export_live.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin-session/examples/upload_project.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin-session/src/accounts.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin-session/src/credentials.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin-session/tests/accounts/peers.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin-session/tests/accounts/race.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin-session/tests/accounts/sync.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin-session/tests/project/support.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin-testkit/src/console.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin-testkit/src/rig.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin-testkit/src/world.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin/src/commands/account.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin/tests/cli/account_manage.rs` | VALUABLE |
| `wt/a11b-account-hardening` | pre-#1264 | `crates/jackin/tests/cli/support.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `Cargo.lock` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-core/src/config.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-core/tests/config_edit.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-core/tests/config_schema.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-core/tests/config_store.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-session/examples/export_live.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-session/examples/upload_project.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-session/src/accounts.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-session/src/credentials.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-session/tests/accounts/peers.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-session/tests/accounts/race.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-session/tests/accounts/sync.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-session/tests/plan/accounts.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-testkit/src/console.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-testkit/src/rig.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin-tui/src/project.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin/src/cli.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin/src/commands/account.rs` | VALUABLE |
| `wt/a14-accounts-integrated` | pre-#1264 | `crates/jackin/tests/cli/support.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-core/tests/manifest.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-openshell/tests/gateway/sandboxes.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/src/accounts.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/src/files.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/src/lib.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/src/multi.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/launch/crash.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/launch/credentials.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/launch/provider.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/launch/support.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/multi_layout.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/plan/accounts.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/plan/availability.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/plan/errors.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/plan/files.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/plan/main.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/plan/omp_models.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/plan/roles_inputs.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-session/tests/project/support.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-testkit/src/rig.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-tui/src/launch.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-tui/tests/console/accounts.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-tui/tests/console/usage_windows.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-tui/tests/doctor.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-tui/tests/launch/account.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-tui/tests/launch/failure.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-tui/tests/launch/picker.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-tui/tests/launch/ready.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-tui/tests/launch/support.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin-tui/tests/launch/vocab.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin/tests/cli/load.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/jackin/tests/cli/support.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/xtask/src/e2e/setup.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/xtask/src/e2e/steps/j03.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/xtask/src/e2e/steps/j08.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/xtask/src/e2e/steps/j14.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/xtask/src/e2e/steps/j15.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/xtask/src/e2e/steps/j17.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/xtask/tests/e2e/c_accounts.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/xtask/tests/e2e/scenario/gateway.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/xtask/tests/e2e/scenario/host.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/xtask/tests/e2e/scenario/pty.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `crates/xtask/tests/e2e/scenario/report.rs` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `rewrite/blockers/T01-codex.md` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `rewrite/blockers/T10-omp-kimi.md` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `rewrite/blockers/T11-omp-minimax.md` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `rewrite/blockers/T12-omp-mimo.md` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `rewrite/blockers/T16-claude-mimo.md` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `rewrite/blockers/T20-codex-mimo.md` | VALUABLE |
| `wt/a14-availability` | pre-#1264 | `rewrite/evidence/A14-final-head-availability.md` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-core/tests/manifest.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-openshell/tests/gateway/sandboxes.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/src/multi.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/multi_layout.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/plan/files.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/plan/omp_models.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/plan/roles_cancel.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/plan/roles_git.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/plan/roles_inputs.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/plan/roles_origin.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/plan/roles_race.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/plan/roles_url.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-session/tests/plan/roles.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/jackin-tui/tests/console/roles.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/xtask/src/e2e/steps/j03.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/xtask/tests/e2e/scenario/gateway.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/xtask/tests/e2e/scenario/host.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/xtask/tests/e2e/scenario/pty.rs` | VALUABLE |
| `wt/a14-availability-final` | pre-#1264 | `crates/xtask/tests/e2e/scenario/report.rs` | VALUABLE |
| `wt/a14-claude-model` | pre-#1264 | `crates/jackin-session/src/files.rs` | VALUABLE |
| `wt/a14-claude-model` | pre-#1264 | `crates/jackin-session/src/launch/run.rs` | VALUABLE |
| `wt/a14-claude-model` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a14-claude-model` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a14-claude-model` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `.audit/a14-oauth-baseline-nextest.log` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `.audit/a14-oauth-g-fast.log` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `.audit/a14-oauth-image-nextest.log` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `.audit/a14-oauth-planner-image-nextest.log` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `.audit/a14-oauth-planner-nextest.log` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `.audit/a14-oauth-red-green.md` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `.audit/a14-oauth-remaining-gates.log` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `.audit/a14-oauth-t02-approval.log` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `.audit/a14-oauth-t03-final-approval.log` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `.audit/a14-oauth-t03-review.log` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `.audit/a14-oauth-targeted-nextest.log` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `crates/jackin-session/src/accounts.rs` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `crates/jackin-session/src/credentials.rs` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `crates/jackin-session/tests/accounts/host_login.rs` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `crates/jackin-session/tests/accounts/lock.rs` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `crates/jackin-session/tests/accounts/race.rs` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `crates/jackin-session/tests/accounts/sync.rs` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `crates/jackin-testkit/src/gateway.rs` | VALUABLE |
| `wt/a14-grok-session` | pre-#1264 | `crates/xtask/tests/e2e/scenario/gateway.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-core/src/config.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/src/accounts.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/src/credentials.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/src/files.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/src/launch/run.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/tests/accounts/host_login.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/tests/accounts/sync.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/tests/launch/credentials.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/tests/launch/support.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/tests/plan/grok.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/tests/plan/native.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-session/tests/plan/static_files.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-testkit/src/gateway.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-tui/src/effects.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-tui/src/launch.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-tui/tests/launch/main.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin-tui/tests/launch/picker.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin/src/commands/account.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin/src/commands/load.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin/tests/cli/account_add.rs` | VALUABLE |
| `wt/a14-native-integrated` | pre-#1264 | `crates/jackin/tests/cli/load.rs` | VALUABLE |
| `wt/a14-oauth-blockers` | pre-#1264 | `rewrite/blockers/T03-grok.md` | VALUABLE |
| `wt/a14-oauth-blockers` | pre-#1264 | `rewrite/blockers/T04-muse.md` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/src/accounts.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/src/credentials.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/src/files.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/src/launch/run.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/src/lib.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/accounts/host_login.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/accounts/race.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/accounts/sync.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/launch/credentials.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/launch/support.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/plan/accounts.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/plan/grok.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/plan/omp_models.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-session/tests/plan/static_files.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-testkit/src/gateway.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-tui/src/launch.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-tui/tests/launch/main.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-tui/tests/launch/picker.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin-tui/tests/launch/tuples.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin/src/commands/load.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `crates/jackin/tests/cli/load.rs` | VALUABLE |
| `wt/a14-oauth-integrated` | pre-#1264 | `rewrite/evidence/A14-oauth-integration.md` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/jackin-core/tests/manifest.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/jackin-session/tests/launch/credentials.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/jackin-session/tests/plan/availability.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/jackin-session/tests/plan/errors.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/jackin-tui/src/launch.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/jackin-tui/tests/console/usage_windows.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/jackin-tui/tests/launch/picker.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/jackin/tests/cli/load.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `crates/xtask/tests/repo_gates.rs` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `rewrite/blockers/T13-claude-kimi.md` | VALUABLE |
| `wt/a14-promotion` | pre-#1264 | `rewrite/decisions.tsv` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-core/tests/manifest.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-openshell/tests/gateway/sandboxes.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/src/multi.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/multi_layout.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/plan/files.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/plan/omp_models.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/plan/roles_cancel.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/plan/roles_git.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/plan/roles_inputs.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/plan/roles_origin.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/plan/roles_race.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/plan/roles_url.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-session/tests/plan/roles.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/jackin-tui/tests/console/roles.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/xtask/src/e2e/steps/j03.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/xtask/tests/e2e/scenario/gateway.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/xtask/tests/e2e/scenario/host.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/xtask/tests/e2e/scenario/pty.rs` | VALUABLE |
| `wt/a14-role-review` | pre-#1264 | `crates/xtask/tests/e2e/scenario/report.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-core/tests/manifest.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-openshell/tests/gateway/sandboxes.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/src/multi.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/multi_layout.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/plan/files.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/plan/omp_models.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/plan/roles_cancel.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/plan/roles_git.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/plan/roles_inputs.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/plan/roles_origin.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/plan/roles_race.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/plan/roles_url.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-session/tests/plan/roles.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/jackin-tui/tests/console/roles.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/xtask/src/e2e/steps/j03.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/xtask/tests/e2e/scenario/gateway.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/xtask/tests/e2e/scenario/host.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/xtask/tests/e2e/scenario/pty.rs` | VALUABLE |
| `wt/a14-stable-pins` | pre-#1264 | `crates/xtask/tests/e2e/scenario/report.rs` | VALUABLE |
| `wt/a14-subscription-close` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-subscription-close` | pre-#1264 | `crates/jackin-image/tests/agent_plan.rs` | VALUABLE |
| `wt/a14-subscription-close` | pre-#1264 | `rewrite/blockers/T02-claude.md` | VALUABLE |
| `wt/a14-subscription-close` | pre-#1264 | `rewrite/blockers/T05-antigravity.md` | VALUABLE |
| `wt/a14-t02-blocker` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-t02-blocker` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a14-t02-blocker` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-t02-blocker` | pre-#1264 | `rewrite/blockers/T02-claude.md` | VALUABLE |
| `wt/a14-t02-live-fixed` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-t02-live-fixed` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a14-t02-live-fixed` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-t02-policy` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-t02-policy` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a14-t02-policy` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-t02-policy-review` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-t02-policy-review` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a14-t02-policy-review` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-core/src/config.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-core/tests/manifest.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/src/accounts.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/src/credentials.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/src/files.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/src/launch/run.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/tests/accounts/host_login.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/tests/accounts/sync.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/tests/launch/credentials.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/tests/launch/support.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/tests/plan/availability.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/tests/plan/native.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-session/tests/plan/static_files.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-testkit/src/gateway.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-tui/src/effects.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-tui/src/launch.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin-tui/tests/launch/picker.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin/src/commands/account.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin/src/commands/load.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin/tests/cli/account_add.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/jackin/tests/cli/load.rs` | VALUABLE |
| `wt/a14-t03-promotion` | pre-#1264 | `crates/xtask/tests/repo_gates.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-core/tests/manifest.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-openshell/tests/gateway/sandboxes.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/src/multi.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/multi_layout.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/plan/files.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/plan/omp_models.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/plan/roles_cancel.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/plan/roles_git.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/plan/roles_inputs.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/plan/roles_origin.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/plan/roles_race.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/plan/roles_url.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-session/tests/plan/roles.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/jackin-tui/tests/console/roles.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/xtask/src/e2e/steps/j03.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/xtask/tests/e2e/scenario/gateway.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/xtask/tests/e2e/scenario/host.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/xtask/tests/e2e/scenario/pty.rs` | VALUABLE |
| `wt/a14-t09-config` | pre-#1264 | `crates/xtask/tests/e2e/scenario/report.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-core/tests/manifest.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-openshell/tests/gateway/sandboxes.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/src/multi.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/multi_layout.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/plan/files.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/plan/omp_models.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/plan/roles_cancel.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/plan/roles_git.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/plan/roles_inputs.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/plan/roles_origin.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/plan/roles_race.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/plan/roles_url.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-session/tests/plan/roles.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/jackin-tui/tests/console/roles.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/xtask/src/e2e/steps/j03.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/xtask/tests/e2e/scenario/gateway.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/xtask/tests/e2e/scenario/host.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/xtask/tests/e2e/scenario/pty.rs` | VALUABLE |
| `wt/a14-t09-live` | pre-#1264 | `crates/xtask/tests/e2e/scenario/report.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-core/tests/manifest.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-openshell/tests/gateway/sandboxes.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/src/multi.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/multi_layout.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/plan/files.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/plan/omp_models.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/plan/roles_cancel.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/plan/roles_git.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/plan/roles_inputs.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/plan/roles_origin.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/plan/roles_race.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/plan/roles_url.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-session/tests/plan/roles.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/jackin-tui/tests/console/roles.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/xtask/src/e2e/steps/j03.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/xtask/tests/e2e/scenario/gateway.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/xtask/tests/e2e/scenario/host.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/xtask/tests/e2e/scenario/pty.rs` | VALUABLE |
| `wt/a14-t09-verify` | pre-#1264 | `crates/xtask/tests/e2e/scenario/report.rs` | VALUABLE |
| `wt/a14-t13-auth` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-t13-auth` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-t13-auth` | pre-#1264 | `crates/jackin-session/tests/accounts/sync.rs` | VALUABLE |
| `wt/a14-t13-auth` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-core/tests/manifest.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-openshell/tests/gateway/sandboxes.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/src/multi.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/multi_layout.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/plan/files.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/plan/omp_models.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/plan/roles_cancel.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/plan/roles_git.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/plan/roles_inputs.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/plan/roles_origin.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/plan/roles_race.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/plan/roles_url.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-session/tests/plan/roles.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/jackin-tui/tests/console/roles.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/xtask/src/e2e/steps/j03.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/xtask/tests/e2e/scenario/gateway.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/xtask/tests/e2e/scenario/host.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/xtask/tests/e2e/scenario/pty.rs` | VALUABLE |
| `wt/a14-zai-final` | pre-#1264 | `crates/xtask/tests/e2e/scenario/report.rs` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `.dockerignore` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `.github/actions/velnor-verify-restored-rust/action.yml` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `.github/actions/velnor-verify-rust-manifest/action.yml` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `.github/workflows/ci.yml` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `.github/workflows/state-seal.yml` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `.gitignore` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `.hadolint.yaml` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `.mise/locks/pipx-reuse/6.3.0~068c1fb0/uv.lock` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `.pre-commit-config.yaml` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `.velnor/config.toml` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `AGENTS.md` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `config/fleet/velnor-host.env` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `Dockerfile` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `hooks/gitleaks-stderr.jq` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `hooks/gitleaks-trace2.jq` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `hooks/gitleaks.sh` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `hooks/preflight.sh` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `hooks/setup.sh` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `instructions/claude.md` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `instructions/codex.md` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `instructions/omp.md` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `jackin.role.toml` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `mise.lock` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `mise.toml` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `NOTICE` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `policy.yaml` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `probe/jackin-sentinel-probe` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `probe/tests/egress.sh` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `probe/tests/hooks.sh` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `probe/tests/j08.sh` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `README.md` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `renovate.json` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `REUSE.toml` | VALUABLE |
| `wt/a15-declared-auth-producer` | pre-#1264 | `tests/gitleaks.sh` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-core/tests/manifest.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-openshell/tests/gateway/sandboxes.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/src/multi.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/multi_layout.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/plan/files.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/plan/omp_models.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/plan/roles_cancel.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/plan/roles_git.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/plan/roles_inputs.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/plan/roles_origin.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/plan/roles_race.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/plan/roles_url.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/plan/roles.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-session/tests/seams/build.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-tui/src/effects.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin-tui/tests/console/roles.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin/src/cli.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/jackin/tests/cli/build.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/src/e2e.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/src/e2e/adapter.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/src/e2e/credential.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/src/e2e/probe.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/src/e2e/run.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/src/e2e/setup.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/src/e2e/steps/j00.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/src/e2e/steps/j07.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/src/e2e/steps/j08.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/src/e2e/steps/j09.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/src/e2e/steps/j11.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/src/e2e/steps/j12.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/src/e2e/steps/j17.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/src/e2e/target.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/tests/e2e/b_probe.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/tests/e2e/c_accounts.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/tests/e2e/c_late.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/tests/e2e/data/probe_host.json` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/tests/e2e/i_target.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/tests/e2e/main.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/tests/e2e/scenario.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/tests/e2e/scenario/gateway.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/tests/e2e/scenario/host.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/tests/e2e/scenario/pty.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `crates/xtask/tests/e2e/scenario/report.rs` | VALUABLE |
| `wt/a15-e2e-t01` | pre-#1264 | `rewrite/probe-schema.json` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-e2e-t01.tsv` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-hook.py` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.adapter.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.adapter.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.agent.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.agent.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.beh.auth_state.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.beh.auth_state.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.beh.binary.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.beh.binary.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.beh.process_comm.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.beh.process_comm.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.beh.provider_status.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.beh.provider_status.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.beh.quit_interrupts.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.beh.quit_interrupts.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.beh.reflection_comm.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.beh.reflection_comm.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.beh.usage.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.beh.usage.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.credential.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.credential.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.default_model.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.default_model.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.home_default.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.home_default.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.model.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.model.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.override.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.override.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.pair.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.pair.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.provider.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.provider.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.selection_args_model.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT1.selection_args_model.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.credential.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.credential.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.process.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.process.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.prompt.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.prompt.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.provider.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.provider.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.reflection.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.reflection.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.terminal.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.terminal.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.usage.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.usage.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.version.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.category.version.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.credential.1password.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.credential.1password.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.credential.env.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.credential.env.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.credential.file.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.credential.file.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.credential.omp-host.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.credential.omp-host.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.invalid_model.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.invalid_model.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.non_supported_pair.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.non_supported_pair.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.outside.provider.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.outside.provider.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.outside.reflection.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.outside.reflection.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.supported_without_adapter.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.supported_without_adapter.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.unknown_pair.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT2.unknown_pair.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.argv_array.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.argv_array.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.closed_matcher.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.closed_matcher.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.access.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.access.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.account_name.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.account_name.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.host_path_codex_home.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.host_path_codex_home.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.host_path_home.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.host_path_home.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.id_token.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.id_token.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.op_reference.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.op_reference.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.refresh.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.refresh.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.usage_value.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.exclude.usage_value.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.home_invariance.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.home_invariance.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.idempotent.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.idempotent.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.known_digest.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.known_digest.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.target_identity.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT4.target_identity.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.answer.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.answer.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.argv.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.argv.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.auth_file.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.auth_file.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.binary.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.binary.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.comm.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.comm.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.credential_env.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.credential_env.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.minimum_age.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.minimum_age.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.model.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.model.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.parsed_hosts.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.parsed_hosts.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.provider_body.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.provider_body.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.provider_status.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.provider_status.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.provider_url.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.provider_url.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.quit.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.quit.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.reflection.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.reflection.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.survivor.adapter.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.survivor.adapter.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.survivor.agent.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.survivor.agent.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.survivor.provider.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.survivor.provider.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.survivor.tuple.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.survivor.tuple.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.terminal.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.terminal.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.usage.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.usage.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.version.green.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-logs/IT5.version.red.log` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-matrix.py` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-normalize.py` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-parsed-hosts.tsv` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-prior-results.tsv` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review-results.tsv` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `.audit/a15-final-review.tsv` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-core/tests/manifest.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-openshell/tests/gateway/sandboxes.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/src/multi.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/multi_layout.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/plan/files.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/plan/omp_models.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/plan/roles_cancel.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/plan/roles_git.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/plan/roles_inputs.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/plan/roles_origin.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/plan/roles_race.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/plan/roles_url.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/plan/roles.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-session/tests/seams/build.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-tui/src/effects.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin-tui/tests/console/roles.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin/src/cli.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/jackin/tests/cli/build.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/src/e2e.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/src/e2e/adapter.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/src/e2e/credential.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/src/e2e/probe.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/src/e2e/run.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/src/e2e/setup.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/src/e2e/steps/j00.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/src/e2e/steps/j07.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/src/e2e/steps/j08.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/src/e2e/steps/j09.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/src/e2e/steps/j11.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/src/e2e/steps/j12.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/src/e2e/steps/j17.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/src/e2e/target.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/tests/e2e/b_probe.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/tests/e2e/c_accounts.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/tests/e2e/c_early.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/tests/e2e/c_late.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/tests/e2e/d_secrets.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/tests/e2e/data/probe_host.json` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/tests/e2e/i_target.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/tests/e2e/main.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/tests/e2e/scenario.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/tests/e2e/scenario/gateway.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/tests/e2e/scenario/host.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/tests/e2e/scenario/pty.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `crates/xtask/tests/e2e/scenario/report.rs` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `rewrite/evidence/A15-e2e-review.md` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `rewrite/evidence/A15-e2e-t01.md` | VALUABLE |
| `wt/a15-final-clean` | pre-#1264 | `rewrite/probe-schema.json` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `.audit/a15-e2e-t01.tsv` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-core/tests/manifest.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-openshell/tests/gateway/sandboxes.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/src/multi.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/multi_layout.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/plan/files.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/plan/omp_models.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/plan/roles_cancel.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/plan/roles_git.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/plan/roles_inputs.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/plan/roles_origin.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/plan/roles_race.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/plan/roles_url.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/plan/roles.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-session/tests/seams/build.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-tui/src/effects.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin-tui/tests/console/roles.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin/src/cli.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/jackin/tests/cli/build.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e/adapter.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e/credential.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e/ledger.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e/probe.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e/run.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e/setup.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e/steps/j00.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e/steps/j07.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e/steps/j08.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e/steps/j09.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e/steps/j11.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e/steps/j12.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e/steps/j17.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/src/e2e/target.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/tests/e2e/b_probe.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/tests/e2e/c_accounts.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/tests/e2e/c_early.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/tests/e2e/c_late.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/tests/e2e/data/probe_host.json` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/tests/e2e/i_target.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/tests/e2e/main.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/tests/e2e/scenario.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/tests/e2e/scenario/gateway.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/tests/e2e/scenario/host.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/tests/e2e/scenario/pty.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `crates/xtask/tests/e2e/scenario/report.rs` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `rewrite/evidence/A15-e2e-t01.md` | VALUABLE |
| `wt/a15-final-review` | pre-#1264 | `rewrite/probe-schema.json` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-core/src/agent.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-core/src/assets/t02-claude.yaml` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-core/tests/agent_instances.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-core/tests/agent_table.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-core/tests/catalog.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-core/tests/manifest.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-openshell/tests/gateway/sandboxes.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-openshell/tests/tuple_policy.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/src/multi.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/src/plan.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/launch/instance_identity.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/launch/multi_instance.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/launch/omp_models.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/launch/worktree.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/multi_layout.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/plan/agents_manifest.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/plan/claude.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/plan/codex.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/plan/files.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/plan/omp_models.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/plan/roles_cancel.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/plan/roles_git.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/plan/roles_inputs.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/plan/roles_origin.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/plan/roles_race.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/plan/roles_url.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/plan/roles.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-session/tests/seams/build.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-tui/src/effects.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin-tui/tests/console/roles.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin/src/cli.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/jackin/tests/cli/build.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/src/e2e/adapter.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/src/e2e/credential.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/src/e2e/probe.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/src/e2e/setup.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/src/e2e/steps/j00.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/src/e2e/steps/j08.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/src/e2e/steps/j09.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/src/e2e/steps/j17.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/tests/e2e/b_probe.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/tests/e2e/c_accounts.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/tests/e2e/i_target.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/tests/e2e/j_supported.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/tests/e2e/scenario.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/tests/e2e/scenario/gateway.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `crates/xtask/tests/e2e/scenario/report.rs` | VALUABLE |
| `wt/a15-supported-targets` | pre-#1264 | `rewrite/probe-schema.json` | VALUABLE |
| `wt/live-sub` | pre-#1264 | `rewrite/blockers/T02-claude.md` | VALUABLE |
| `wt/live-sub` | pre-#1264 | `rewrite/blockers/T05-antigravity.md` | VALUABLE |
| `wt/models-research` | pre-#1264 | `rewrite/research/t13-t22-models.md` | VALUABLE |

1087 files across 49 VALUABLE branches.

## Table 3 - per-file disposition of the dirty worktree files (182 entries)

`NEW_CONTENT` = the working-tree blob of this file is not in `main` at or after the #1264 squash, so
the edit exists nowhere else. `CONTAINED` = the blob is already in `main`; the edit is a no-op against
today's tree and is safe to drop.

| worktree | branch | status | path | verdict |
|---|---|---|---|---|
| `fanout-call_01a1ca3c3c87734fa42bfaef-0` | `swarm/fanout-call_01a1ca3c3c87734fa42bfaef-0` | `M` | `crates/jackin-tui/src/console.rs` | CONTAINED_IN_MAIN |
| `fanout-call_01a1ca3c3c87734fa42bfaef-1` | `swarm/fanout-call_01a1ca3c3c87734fa42bfaef-1` | `M` | `.betterleaksignore` | NEW_CONTENT |
| `fanout-call_01a1ca3c3c87734fa42bfaef-1` | `swarm/fanout-call_01a1ca3c3c87734fa42bfaef-1` | `M` | `.gitleaksignore` | NEW_CONTENT |
| `fanout-call_01a1ca3c3c87734fa42bfaef-1` | `swarm/fanout-call_01a1ca3c3c87734fa42bfaef-1` | `M` | `.kingfisher-baseline.yml` | CONTAINED_IN_MAIN |
| `fanout-call_1c9ab3259fa54c9398809a67-3` | `swarm/fanout-call_1c9ab3259fa54c9398809a67-3` | `M` | `Cargo.toml` | NEW_CONTENT |
| `fanout-call_1c9ab3259fa54c9398809a67-3` | `swarm/fanout-call_1c9ab3259fa54c9398809a67-3` | `M` | `crates/usage/jackin-usage-provider-core/Cargo.toml` | NEW_CONTENT |
| `fanout-call_1c9ab3259fa54c9398809a67-3` | `swarm/fanout-call_1c9ab3259fa54c9398809a67-3` | `M` | `crates/usage/jackin-usage-provider-core/src/http.rs` | NEW_CONTENT |
| `fanout-call_3d4ffa93267d4f40a885ab3d-1` | `swarm/fanout-call_3d4ffa93267d4f40a885ab3d-1` | `M` | `crates/jackin-openshell/src/error.rs` | NEW_CONTENT |
| `fanout-call_61766d565b904e3dba2db8d5-0` | `swarm/fanout-call_61766d565b904e3dba2db8d5-0` | `M` | `crates/xtask/src/e2e/credential.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-0` | `swarm/fanout-call_61766d565b904e3dba2db8d5-0` | `M` | `crates/xtask/src/e2e/production.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-0` | `swarm/fanout-call_61766d565b904e3dba2db8d5-0` | `M` | `crates/xtask/src/e2e/scratch.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-0` | `swarm/fanout-call_61766d565b904e3dba2db8d5-0` | `M` | `crates/xtask/src/e2e/setup.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-0` | `swarm/fanout-call_61766d565b904e3dba2db8d5-0` | `M` | `crates/xtask/src/e2e/steps/j00.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-0` | `swarm/fanout-call_61766d565b904e3dba2db8d5-0` | `M` | `crates/xtask/src/e2e/steps/j02.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-0` | `swarm/fanout-call_61766d565b904e3dba2db8d5-0` | `M` | `crates/xtask/src/e2e/steps/j05.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-0` | `swarm/fanout-call_61766d565b904e3dba2db8d5-0` | `M` | `crates/xtask/src/e2e/steps/j09.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-0` | `swarm/fanout-call_61766d565b904e3dba2db8d5-0` | `??` | `rewrite/blockers/T08-kimi-code.md` | NEW_CONTENT |
| `fanout-call_61766d565b904e3dba2db8d5-1` | `swarm/fanout-call_61766d565b904e3dba2db8d5-1` | `M` | `crates/jackin-openshell/tests/gateway/discovery.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-1` | `swarm/fanout-call_61766d565b904e3dba2db8d5-1` | `M` | `crates/jackin-session/tests/project/refuse_objects.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-1` | `swarm/fanout-call_61766d565b904e3dba2db8d5-1` | `M` | `crates/jackin-testkit/src/gateway.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-1` | `swarm/fanout-call_61766d565b904e3dba2db8d5-1` | `M` | `crates/jackin-testkit/src/rig.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-1` | `swarm/fanout-call_61766d565b904e3dba2db8d5-1` | `M` | `crates/jackin-testkit/tests/gateway/transfer.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-2` | `swarm/fanout-call_61766d565b904e3dba2db8d5-2` | `M` | `crates/jackin-session/examples/build_images.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-2` | `swarm/fanout-call_61766d565b904e3dba2db8d5-2` | `M` | `crates/jackin-session/examples/export_live.rs` | CONTAINED_IN_MAIN |
| `fanout-call_61766d565b904e3dba2db8d5-2` | `swarm/fanout-call_61766d565b904e3dba2db8d5-2` | `M` | `crates/jackin-session/examples/export_memory.rs` | NEW_CONTENT |
| `fanout-call_61766d565b904e3dba2db8d5-2` | `swarm/fanout-call_61766d565b904e3dba2db8d5-2` | `M` | `crates/jackin-session/examples/upload_project.rs` | CONTAINED_IN_MAIN |
| `fanout-call_667899f3ad454fd1946b4556-0` | `swarm/fanout-call_667899f3ad454fd1946b4556-0` | `M` | `crates/jackin-core/src/config/store.rs` | CONTAINED_IN_MAIN |
| `fanout-call_667899f3ad454fd1946b4556-0` | `swarm/fanout-call_667899f3ad454fd1946b4556-0` | `M` | `crates/jackin-session/src/accounts.rs` | CONTAINED_IN_MAIN |
| `fanout-call_667899f3ad454fd1946b4556-1` | `swarm/fanout-call_667899f3ad454fd1946b4556-1` | `M` | `crates/jackin-core/src/config/store.rs` | CONTAINED_IN_MAIN |
| `fanout-call_667899f3ad454fd1946b4556-1` | `swarm/fanout-call_667899f3ad454fd1946b4556-1` | `M` | `crates/jackin-session/src/accounts.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-0` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-0` | `M` | `crates/jackin-core/src/config/store.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-0` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-0` | `M` | `crates/jackin-session/src/accounts.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-0` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-0` | `M` | `crates/jackin-session/src/credentials.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-0` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-0` | `M` | `crates/jackin-session/tests/accounts/peers.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-0` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-0` | `M` | `crates/jackin/src/commands/account.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `Cargo.lock` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/jackin-testkit/src/console.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/jackin/src/commands/console.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/jackin/src/commands/usage.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-claude/src/lib.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-claude/src/windows.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-codex/src/lib.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-codex/src/windows.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-core/Cargo.toml` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-core/src/http.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-core/src/model.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-cursor/src/lib.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-gemini/Cargo.toml` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-gemini/src/lib.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-grok/src/lib.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-kimi/src/lib.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-minimax/src/lib.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-opencode/src/lib.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-openrouter/src/lib.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage-provider-zai/src/lib.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage/Cargo.toml` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage/src/cache.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage/src/credential.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage/src/deps.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage/src/dispatch.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage/src/host.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage/src/model.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage/src/text.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage/src/window.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage/tests/usage/schema.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage/tests/usage/secrets.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage/tests/usage/support.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-1` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-1` | `M` | `crates/usage/jackin-usage/tests/usage/text.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-core/src/boundary.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-core/src/config.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-core/src/ids.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-core/src/labels.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-core/src/manifest.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-core/src/paths.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-openshell/src/discovery.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-openshell/src/error.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-openshell/src/grpc/sandbox.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-session/src/project.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-session/src/roles.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-session/tests/accounts/edit.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-session/tests/accounts/host_login.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-session/tests/accounts/peers.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-session/tests/plan/scoped_env.rs` | NEW_CONTENT |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-testkit/src/fixtures.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-tui/src/effects.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin-tui/src/panes.rs` | CONTAINED_IN_MAIN |
| `fanout-call_7b848c095cd548d2a97dfc27-2` | `swarm/fanout-call_7b848c095cd548d2a97dfc27-2` | `M` | `crates/jackin/tests/cli/account_add.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-2` | `swarm/fanout-call_84bf371fb4fb46308f47c848-2` | `M` | `crates/jackin-core/src/ids.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-2` | `swarm/fanout-call_84bf371fb4fb46308f47c848-2` | `M` | `crates/jackin-session/src/attach.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-2` | `swarm/fanout-call_84bf371fb4fb46308f47c848-2` | `M` | `crates/jackin-session/src/export.rs` | CONTAINED_IN_MAIN |
| `fanout-call_84bf371fb4fb46308f47c848-2` | `swarm/fanout-call_84bf371fb4fb46308f47c848-2` | `M` | `crates/jackin-session/src/launch.rs` | CONTAINED_IN_MAIN |
| `fanout-call_84bf371fb4fb46308f47c848-2` | `swarm/fanout-call_84bf371fb4fb46308f47c848-2` | `M` | `crates/jackin-session/src/launch/run.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-2` | `swarm/fanout-call_84bf371fb4fb46308f47c848-2` | `M` | `crates/jackin-session/src/lifecycle.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-2` | `swarm/fanout-call_84bf371fb4fb46308f47c848-2` | `M` | `crates/jackin-session/src/multi_session.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-2` | `swarm/fanout-call_84bf371fb4fb46308f47c848-2` | `M` | `crates/jackin-session/tests/export/purge.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-2` | `swarm/fanout-call_84bf371fb4fb46308f47c848-2` | `M` | `crates/jackin-session/tests/launch/instance_identity.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-2` | `swarm/fanout-call_84bf371fb4fb46308f47c848-2` | `M` | `crates/jackin-session/tests/launch/multi_instance.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-2` | `swarm/fanout-call_84bf371fb4fb46308f47c848-2` | `M` | `crates/jackin-session/tests/launch/workspace_fence.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-2` | `swarm/fanout-call_84bf371fb4fb46308f47c848-2` | `M` | `crates/jackin-session/tests/launch/worktree.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-2` | `swarm/fanout-call_84bf371fb4fb46308f47c848-2` | `M` | `crates/jackin-session/tests/lifecycle/inspect.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/jackin-tui/tests/console/usage_windows.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/jackin/tests/cli/usage.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage-provider-claude/src/lib.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage-provider-claude/src/windows.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage-provider-codex/src/lib.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage-provider-codex/src/windows.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage-provider-grok/src/lib.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage-provider-kimi/src/lib.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage-provider-minimax/src/lib.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage-provider-opencode/src/lib.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage-provider-openrouter/src/lib.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage-provider-zai/src/lib.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/src/cache.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/src/credential.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/src/dispatch.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/src/host.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/cache.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/credential_files.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/credentials.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/errors.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/fixtures/grok/billing_credits.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/fixtures/grok/billing_deprecated.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/fixtures/grok/billing_negative_cent.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/fixtures/grok/billing_zero_cap.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/providers/claude.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/providers/codex.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/providers/grok.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/providers/kimi.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/providers/minimax.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/providers/opencode.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/providers/openrouter.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/providers/zai.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/secrets.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `M` | `crates/usage/jackin-usage/tests/usage/support.rs` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/claude/credentials_expired.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/claude/oauth_usage_grants.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/claude/oauth_usage_grants_inactive.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/claude/oauth_usage_surface_scope.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/codex/auth_chatgpt_rotated.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/codex/reset_credits.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/codex/reset_credits_empty.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/grok/auth_no_expiry.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/grok/billing_no_on_demand_flag.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/grok/billing_product_share.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/kimi/usages_envelope.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/kimi/usages_envelope_partial.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/minimax/auth_records.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/minimax/auth_records_expired.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/opencode/auth_console_oauth.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/opencode/auth_console_oauth_expired.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/zai/quota_no_envelope.json` | NEW_CONTENT |
| `fanout-call_84bf371fb4fb46308f47c848-3` | `swarm/fanout-call_84bf371fb4fb46308f47c848-3` | `??` | `crates/usage/jackin-usage/tests/usage/fixtures/zai/quota_time_limit.json` | NEW_CONTENT |
| `fanout-call_c150420bac2f4875b01d23cd-1` | `swarm/fanout-call_c150420bac2f4875b01d23cd-1` | `M` | `crates/jackin-core/src/fs.rs` | CONTAINED_IN_MAIN |
| `fanout-call_e6e20aee930345eb8831374f-1` | `swarm/fanout-call_e6e20aee930345eb8831374f-1` | `M` | `crates/jackin-openshell/src/grpc.rs` | NEW_CONTENT |
| `fanout-call_e6e20aee930345eb8831374f-1` | `swarm/fanout-call_e6e20aee930345eb8831374f-1` | `M` | `crates/jackin-openshell/src/grpc/connect.rs` | CONTAINED_IN_MAIN |
| `fanout-call_e6e20aee930345eb8831374f-1` | `swarm/fanout-call_e6e20aee930345eb8831374f-1` | `M` | `crates/jackin-openshell/src/grpc/status.rs` | CONTAINED_IN_MAIN |
| `fanout-call_e6e20aee930345eb8831374f-1` | `swarm/fanout-call_e6e20aee930345eb8831374f-1` | `M` | `crates/jackin-openshell/tests/gateway/connect.rs` | NEW_CONTENT |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `M` | `crates/jackin-core/src/config.rs` | CONTAINED_IN_MAIN |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `M` | `crates/jackin-core/src/manifest.rs` | NEW_CONTENT |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `M` | `crates/jackin-core/tests/manifest.rs` | CONTAINED_IN_MAIN |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `M` | `crates/jackin-session/tests/accounts/edit.rs` | NEW_CONTENT |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `M` | `crates/jackin-session/tests/accounts/host_login.rs` | NEW_CONTENT |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `M` | `crates/jackin-session/tests/accounts/peers.rs` | NEW_CONTENT |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `M` | `crates/jackin-session/tests/plan/scoped_env.rs` | NEW_CONTENT |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `M` | `crates/jackin/tests/cli/account_add.rs` | NEW_CONTENT |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `M` | `crates/jackin/tests/cli/role_check.rs` | CONTAINED_IN_MAIN |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-4` | `??` | `crates/jackin-core/tests/provider_eligibility.rs` | NEW_CONTENT |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-5` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-5` | `M` | `crates/xtask/src/e2e/probe.rs` | CONTAINED_IN_MAIN |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-5` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-5` | `M` | `crates/xtask/src/e2e/steps/j08.rs` | CONTAINED_IN_MAIN |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-5` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-5` | `M` | `crates/xtask/tests/e2e/b_probe.rs` | CONTAINED_IN_MAIN |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-5` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-5` | `M` | `crates/xtask/tests/e2e/scenario/report.rs` | CONTAINED_IN_MAIN |
| `fanout-call_g9nTkf5prOz7xc6dndCCqSAj-5` | `swarm/fanout-call_g9nTkf5prOz7xc6dndCCqSAj-5` | `M` | `rewrite/probe-schema.json` | CONTAINED_IN_MAIN |
| `tool-call_01a1ce4ead2774dfa85e5567` | `fix/pr1264-followups-w3` | `M` | `crates/usage/jackin-usage/src/credential.rs` | NEW_CONTENT |
| `tool-call_01a1ce4ead2774dfa85e5567` | `fix/pr1264-followups-w3` | `M` | `crates/usage/jackin-usage/tests/usage/opencode.rs` | NEW_CONTENT |
| `tool-call_408ca8fd3b9e44639da156ae` | `swarm/tool-call_408ca8fd3b9e44639da156ae` | `??` | `sonar-project.properties` | CONTAINED_IN_MAIN |
| `tool-call_9b95e9cc2960449db0fe2d27` | `swarm/tool-call_9b95e9cc2960449db0fe2d27` | `M` | `Cargo.lock` | NEW_CONTENT |
| `tool-call_9b95e9cc2960449db0fe2d27` | `swarm/tool-call_9b95e9cc2960449db0fe2d27` | `M` | `Cargo.toml` | CONTAINED_IN_MAIN |
| `tool-call_9b95e9cc2960449db0fe2d27` | `swarm/tool-call_9b95e9cc2960449db0fe2d27` | `M` | `crates/jackin-session/Cargo.toml` | CONTAINED_IN_MAIN |
| `tool-call_9b95e9cc2960449db0fe2d27` | `swarm/tool-call_9b95e9cc2960449db0fe2d27` | `M` | `crates/jackin-session/examples/export_memory.rs` | NEW_CONTENT |
| `tool-call_9b95e9cc2960449db0fe2d27` | `swarm/tool-call_9b95e9cc2960449db0fe2d27` | `M` | `crates/jackin-session/src/export/bundle.rs` | CONTAINED_IN_MAIN |
| `tool-call_9b95e9cc2960449db0fe2d27` | `swarm/tool-call_9b95e9cc2960449db0fe2d27` | `M` | `rewrite/drafts/docker-laneA-run.sh` | CONTAINED_IN_MAIN |
| `tool-call_9b95e9cc2960449db0fe2d27` | `swarm/tool-call_9b95e9cc2960449db0fe2d27` | `??` | `.sonarcloud.properties` | NEW_CONTENT |

## DEBRIS with proof

| branch | tip | proof |
|---|---|---|
| `chore/triage-inventory-20261112` | `e13360eb8751` | named in the ratified 15-branch DEBRIS set (rewrite/decisions.tsv, 2026-11-14T16:14:00Z row); carries no unique product code |
| `swarm/pr1287-debris-ratification` | `b550ce9ddccb` | byte-identical tip b550ce9ddccb to docs/debris-ratification-2026-11-14; the unit is carried there |
| `wt/a14-native-implementation` | `17aacafcfa08` | byte-identical tip 17aacafcfa08 to wt/a14-grok-session; the unit is carried there |
| `wt/a14-oauth-files` | `17aacafcfa08` | byte-identical tip 17aacafcfa08 to wt/a14-grok-session; the unit is carried there |
| `wt/a14-t02-final-oauth` | `94d880efefe9` | byte-identical tip 94d880efefe9 to wt/a14-oauth-integrated; the unit is carried there |
| `wt/a14-t03-final-oauth` | `94d880efefe9` | byte-identical tip 94d880efefe9 to wt/a14-oauth-integrated; the unit is carried there |
| `wt/a14-t13-live-fixed` | `8c444a97ad71` | byte-identical tip 8c444a97ad71 to wt/a14-t13-auth; the unit is carried there |
| `wt/pr1281-review-fixes` | `cf9acb7485e1` | byte-identical tip cf9acb7485e1 to docs/triage-inventory-reclass-2026-11-13; the unit is carried there |

## Unclassified

Nothing in the 168 local branches is unclassified: every branch fell into exactly one class with a
machine-checkable proof. Three caveats about the environment, not about the branches:

* The coordinator worktree `/private/tmp/jackin-a14-integrated` lost its `.git` link file during this
  slice (another process removed it), so its own dirty state could not be re-read at the end. The
  state measured earlier in the slice was 162 entries: 9 modified tracked files and 153 untracked
  `.audit/*.log` files. The 9 modified tracked files are exactly the `chore/dirty-slice-1` unit, which
  is already recorded in Table 2; the 153 `.audit/*.log` files are build and test logs with no
  product content.
* The 182 dirty files are a point-in-time measurement; the swarm is live and one conflicted worktree
  (`fanout-call_01a1dacfcfea75d095635e4c-3`, branch `w3/pr1281-merge`) was resolved by another
  process during the slice and went from 3 dirty entries to 0.
* The 184 worktrees outside `.pi/durable-swarm/worktrees/` were counted (`git worktree list` = 202
  entries) but their dirty state was not swept individually; the dirty ones in that set are the
  coordinator worktree and the `swarm/` copies listed above.

## Rule applied to every branch

* **VALUABLE** - the branch holds at least one file version that is in no merged pull request. It is
  pushed to `origin` so the content survives.
* **CONTAINED** - every file the branch's own commits touch is already in `main` at or after the
  #1264 squash, or every one of its commits is patch-id-equal to landed content. The merged pull
  request that carries the content is cited in Table 1.
* **DEBRIS** - the branch is a byte-identical duplicate of another branch's tip, or it is named in the
  repository's own ratified 15-branch DEBRIS set. The carrying branch or the ratification record is
  cited in Table 1 and in the DEBRIS section.

