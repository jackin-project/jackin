> Each entry below is one guarded deletion. Run it as `git push origin --force-with-lease=refs/heads/BRANCH:EXPECTED --delete BRANCH`, where BRANCH and EXPECTED come from the entry. The lease refuses the delete when the branch has moved. Nothing here has been run, and every deletion needs the operator's authorization.

# Deletion final v2: jackin-project/jackin remote branches

**Status: read-only audit. Nothing was deleted, pushed, committed, closed or changed.** Every command in section 2 is NOT EXECUTED.

**Snapshot:** `git fetch --all --prune` ran at 2026-11-25 05:31 UTC, then `git ls-remote --heads origin`. origin has 158 heads including `main` (v1 had 163). `origin/main` is `f151478146` (#1324 merged). Each current SHA below is the value from ls-remote, and the lease in section 2 uses it.

**Inputs:** `deletion-final.md` (v1 rows and verdicts), the six `retire-verify-*.md` files, `../orchestrator/ledger-final.md`, `gh pr list --state all` (300 PRs), closing comments of closed PRs, the plan's donor table in `/Users/donbeave/Projects/jackin-project/jackin/jackin-branch-porting-plan.md` section 1, and `git worktree list` with per-worktree status.

## Totals

| Class | Count | Meaning |
|---|---:|---|
| DELETE-NOW | 130 | v1 retire class, verified SAFE in a verify file, no open PR, not a donor, unprotected, not checked out in a worktree, SHA unchanged since v1 |
| NEWLY-DELETABLE | 8 | (a) 6 this-run fix/test branches whose PR is CLOSED with "Landed in #…" or MERGED, ref equals the PR head; (b) 2 donor refs (P08 only) whose P-task is merged |
| KEEP | 20 | main, open PRs (#1287, #1316, #1317, #1322, #1325), donors still serving open P11/P15/P01/P02, other-session refs, group A refs |
| MOVED | 10 | SHA differs from v1. Every moved ref was re-checked: each current SHA equals its PR head (or is main). Dispositions are in section 1 |

Total on origin now: 158 (including main). Compared with v1: 6 this-run branches were deleted on origin (fix/p05, fix/p06, fix/p11, fix/p13, fix/p16, fix/p21), and `fix/deferred-review-followups` (#1322, open) is new. The v1 GONE rows (feat/usage-accounts-ux, fix/p04, fix/p09, fix/p18, fix/p19) are still absent.

## 1. Every remote head on origin now

Columns: current SHA (ls-remote, 12 chars), SHA recorded in v1, class, MOVED (current differs from v1), basis.

| branch | current | v1 | class | MOVED | basis |
|---|---|---|---|---|---|
| `backup/w3-local-usage` | f0aa365be70a | f0aa365be70a | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `chore/dirty-slice-1` | 72401bf5e78c | 72401bf5e78c | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `chore/dirty-slice-accounting-2026-11-14` | fe4fcb3a11d1 | fe4fcb3a11d1 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `chore/restore-1283-remainder` | e519e06e4885 | e519e06e4885 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `chore/triage-inventory-20261112` | e13360eb8751 | e13360eb8751 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `codex/openshell-integration-checkpoint-20261108-122311` | 6965f5d1c222 | 6965f5d1c222 | KEEP |  | donor; open P11 (#1325), P15 (#1316) |
| `codex/openshell-verified-checkpoint-20261108` | ce119545c6e1 | ce119545c6e1 | KEEP |  | donor; open P11 (#1325), P15 (#1316) |
| `docs/debris-ratification-2026-11-14` | 5044fc033e0e | 5044fc033e0e | KEEP |  | open PR #1287 (not this run); keep until reconciled |
| `docs/native-records-port-2026-11-15` | e3fdf0329c88 | e3fdf0329c88 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `docs/pr1228-replacement-20261026` | 912fe737e08f | 912fe737e08f | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `docs/triage-inventory-postmerge-2026-11-15` | 48d495d01b95 | 48d495d01b95 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `docs/triage-inventory-reclass-2026-11-13` | cf9acb7485e1 | cf9acb7485e1 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `feat/native-usage-accounts` | 8b5b99cd2a28 | 8b5b99cd2a28 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `feat/probe-j07-launcher-identity` | 6214bd6dde90 | 6214bd6dde90 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `feature/native-usage-accounts` | f50d6b0af6c2 | f50d6b0af6c2 | KEEP |  | checked out in jackin-native-usage; other session |
| `fix/1287-doc-a` | 7f13b545154f | 7f13b545154f | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `fix/1287-doc-b` | bc5dbe048d53 | bc5dbe048d53 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `fix/cli-review-fixes` | b95cd9288837 | b95cd9288837 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `fix/core-plan-review` | b172385c87f7 | b172385c87f7 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `fix/deferred-review-followups` | 29f2c08b146f | none | KEEP |  | open PR #1322 (this run) |
| `fix/p01-store-capture` | db33e84a7196 | 4338566da9e3 | KEEP | yes | open PR #1317 |
| `fix/p02-exact-openrouter-money` | 56e4d2586b40 | a9f3631b690d | KEEP | yes | open PR #1325 |
| `fix/p03-cancellation-recovery` | b6f14fa99dd7 | abf24394c4f1 | NEWLY-DELETABLE | yes | PR #1314 CLOSED, landed in #1324 f151478146; ref == PR head |
| `fix/p07-fleet-observation` | 8ef2570e830e | 3c39bbbf2a16 | NEWLY-DELETABLE | yes | PR #1319 CLOSED, landed in #1324 f151478146; ref == PR head |
| `fix/p08-role-manifest-refresh` | dd68ebbb8df9 | bf102c1856a1 | NEWLY-DELETABLE | yes | PR #1309 CLOSED, landed in #1321 51f2d808; ref == PR head |
| `fix/p12-role-locks` | dc46590fa053 | 7f321a34c2e5 | NEWLY-DELETABLE | yes | PR #1305 CLOSED, landed in #1321 51f2d808; ref == PR head |
| `fix/p15-final-image-paths` | 9485838605e2 | e46889836c97 | KEEP | yes | open PR #1316 |
| `fix/p17-sandbox-write` | ec9df1407ef1 | aadef4e155fd | NEWLY-DELETABLE | yes | PR #1308 CLOSED, landed in #1321 51f2d808; ref == PR head |
| `fix/port-1264-review-findings-2026-11-15` | ca2f48f6bd87 | ca2f48f6bd87 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `fix/pr1264-followups-w3` | 05549f29aab9 | 05549f29aab9 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `fix/pr1264-review-followups` | d79ad9b29e10 | d79ad9b29e10 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `fix/roles-review-impl` | 04471245d9b9 | 04471245d9b9 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `fix/roles-review-impl-v2` | 3d2401c1b34b | 3d2401c1b34b | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `fix/secrets-scan-squash-1264` | 13d9038ce124 | 13d9038ce124 | KEEP |  | group A, no verify file (pre-run) |
| `fix/session-purge-git-lock` | 81de70c64f66 | 81de70c64f66 | KEEP |  | checked out in jackin-purge-lock; PR #1297 closed (other session) |
| `fix/typed-model-selection` | 0aaf7a411ce0 | 0aaf7a411ce0 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `fix/usage-review-a` | f6a83f1167ec | f6a83f1167ec | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `fix/usage-review-b` | e37b59e4beb8 | e37b59e4beb8 | KEEP |  | donor; open P01 (#1317) |
| `main` | f151478146fb | b4ee3acd7e96 | KEEP | yes | default branch, protected |
| `preserve/https-pr1279` | 5302848e22ea | 5302848e22ea | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `preserve/jackin-work-scratch` | 74b9025a0f5f | 74b9025a0f5f | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `preserve/orch-1287-scratch` | 4451a50b8adc | 4451a50b8adc | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `preserve/p0.6-prototype-20261109` | 63b2976a53df | 63b2976a53df | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `preserve/pi-durable-swarm-notes` | d901ab2393e8 | d901ab2393e8 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `preserve/pr1264-merge` | bbe6b5bb70b3 | bbe6b5bb70b3 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/pull-1285-head` | 71fa7744b4b2 | 71fa7744b4b2 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `preserve/session-boundaries-20261109` | 16de1eedff91 | 16de1eedff91 | KEEP |  | donor; open P11 (#1325), P15 (#1316) |
| `preserve/stash-fix-roles-review-impl` | befcdea355b2 | befcdea355b2 | DELETE-NOW |  | SAFE (retire-verify-1.md) |
| `preserve/stash-w8-port-feature-openshell-native` | f2da2d2159c0 | f2da2d2159c0 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1aedd-52c9-7ec1-8c5f-9a41cce8d991-` | 62b483ee4e75 | 62b483ee4e75 | DELETE-NOW |  | SAFE (retire-verify-subagent-v2.md) |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1aefc-957f-7911-b340-d5b4968bca6e-` | 8c427113e53e | 8c427113e53e | DELETE-NOW |  | SAFE (retire-verify-subagent-v2.md) |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af34-b536-70b0-95c7-45cd4e71d44c-` | e1dceb179bef | e1dceb179bef | DELETE-NOW |  | SAFE (retire-verify-subagent-v2.md) |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af57-e5e9-7443-ad58-f6e04b26b0ae-` | 86fc1d637aee | 86fc1d637aee | DELETE-NOW |  | SAFE (retire-verify-subagent-v2.md) |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af6f-49a5-76c0-b265-0f5224993db2-` | 1dbe3f65a1e2 | 1dbe3f65a1e2 | DELETE-NOW |  | SAFE (retire-verify-subagent-v2.md) |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af7b-2b84-7a62-93f4-f7b4c789469c-` | 16005c405aae | 16005c405aae | DELETE-NOW |  | SAFE (retire-verify-subagent-v2.md) |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af8d-4412-7bd2-83b0-008313c1f23a-` | 15c12d19e544 | 15c12d19e544 | DELETE-NOW |  | SAFE (retire-verify-subagent-v2.md) |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afa2-1284-7d01-b72d-28eff9b2e124-` | 1c1cabdae44f | 1c1cabdae44f | KEEP |  | donor; open P15 (#1316) |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afaa-b762-7170-a3e6-9c9f07aa9035-` | 8d1b528a680d | 8d1b528a680d | DELETE-NOW |  | SAFE (retire-verify-subagent-v2.md) |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afcb-0959-7ce1-bc99-05490e535252-` | c5a98a0870c9 | c5a98a0870c9 | DELETE-NOW |  | SAFE (retire-verify-subagent-v2.md) |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afdb-bcd8-7a43-8ac3-0cfcc4d54763-` | 1190c7efac65 | 1190c7efac65 | DELETE-NOW |  | SAFE (retire-verify-subagent-v2.md) |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afe7-dbcc-7580-ad46-c84d6e0ab127-` | 43fc09eeaf78 | 43fc09eeaf78 | DELETE-NOW |  | SAFE (retire-verify-subagent-v2.md) |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1aff2-7c8f-7c91-8944-99e86e1939a5-` | f424cce6020c | f424cce6020c | DELETE-NOW |  | SAFE (retire-verify-subagent-v2.md) |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1b002-7a9c-70b2-9102-7a47e94b1726-` | 5e1f8c2dbac1 | 5e1f8c2dbac1 | DELETE-NOW |  | SAFE (retire-verify-subagent-v2.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/0` | f6c2bff00d62 | f6c2bff00d62 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/1` | c9a4ad50cad8 | c9a4ad50cad8 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/10` | 1e928c0af82c | 1e928c0af82c | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/11` | 8d1e781bf921 | 8d1e781bf921 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/12` | 10034b2a4d77 | 10034b2a4d77 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/13` | 52a1aabe06e6 | 52a1aabe06e6 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/14` | 517097494820 | 517097494820 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/15` | 7f1f0f92c40a | 7f1f0f92c40a | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/16` | 7fc4466a93bb | 7fc4466a93bb | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/17` | 9156f83145b3 | 9156f83145b3 | NEWLY-DELETABLE |  | donor for P08 only; PR #1309 CLOSED, landed in #1321 51f2d808; no verify file |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/18` | 1dd6b8e4ed5f | 1dd6b8e4ed5f | NEWLY-DELETABLE |  | donor for P08 only; PR #1309 CLOSED, landed in #1321 51f2d808; no verify file |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/19` | abe950fac0fa | abe950fac0fa | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/2` | d9c34f15fef0 | d9c34f15fef0 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/20` | 146052403541 | 146052403541 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/21` | e99b4a424c7b | e99b4a424c7b | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/22` | d7a22844cd19 | d7a22844cd19 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/23` | cd4fc9ee48b4 | cd4fc9ee48b4 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/24` | fe7c87c9e242 | fe7c87c9e242 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/25` | 22cd55144654 | 22cd55144654 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/26` | d7a1c0ef1020 | d7a1c0ef1020 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/27` | 1ae7d77013d6 | 1ae7d77013d6 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/28` | 814b61d338d8 | 814b61d338d8 | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/29` | 4f4ea8bf5aab | 4f4ea8bf5aab | DELETE-NOW |  | SAFE (retire-verify-t3-body.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/3` | 2c88c91d5928 | 2c88c91d5928 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/30` | 65f6a4008150 | 65f6a4008150 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/4` | 557429f3be14 | 557429f3be14 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/5` | c8ad0d23823a | c8ad0d23823a | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/6` | 8519884113da | 8519884113da | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/7` | 94d896696096 | 94d896696096 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/8` | 674353bbf9ce | 674353bbf9ce | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/9` | 4940d2902cc7 | 4940d2902cc7 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/t3/YjE5NTdmNGViMzdhNDM3ZTYwZjI1N2UwODdiYzZiMDA/0` | 41a002522ee1 | 41a002522ee1 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/t3/YjE5NTdmNGViMzdhNDM3ZTYwZjI1N2UwODdiYzZiMDA/1` | 6cd95820272d | 6cd95820272d | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/t3/YjE5NTdmNGViMzdhNDM3ZTYwZjI1N2UwODdiYzZiMDA/2` | 8bc03fa83b32 | 8bc03fa83b32 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/t3/YjE5NTdmNGViMzdhNDM3ZTYwZjI1N2UwODdiYzZiMDA/3` | bc17efa2ba8b | bc17efa2ba8b | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/w1-fixes-20261109` | 78d8b5f02f3f | 78d8b5f02f3f | KEEP |  | donor; open P15 (#1316) |
| `preserve/w2-impl-strong-e2e-zai` | b8b0cda4990f | b8b0cda4990f | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/w6-fix-strong-` | 4aa4a01eabc3 | 4aa4a01eabc3 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/worktree-` | c09004bba05f | c09004bba05f | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `preserve/wt-` | 3f3f6b826c5f | 3f3f6b826c5f | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `recovery/branches-nightly-compat-20261029` | 9827b3adcf7b | 9827b3adcf7b | KEEP |  | group A, no verify file (pre-run) |
| `recovery/openrouter-exact-money-20261027` | 6ebbe14490b2 | 6ebbe14490b2 | KEEP |  | donor; open P02 (#1325) |
| `recovery/pr1230-blob-auth-77b399f1-20261027` | 77b399f12426 | 77b399f12426 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `recovery/stash-2f8cc42b-20261023` | e5e6d1fd27ef | e5e6d1fd27ef | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `recovery/stash-2fb075f8-20261023` | 4d9540abf301 | 4d9540abf301 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `recovery/stash-88f5643c-20261023` | ddf8861f4564 | ddf8861f4564 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `recovery/stash-adoption-6a0d2f03-20261025` | ade401e79e3e | ade401e79e3e | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `recovery/stash-be51ff69-20261023` | 9cdf189b812c | 9cdf189b812c | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `recovery/stash-d808d094-20261023` | 00f98f32514a | 00f98f32514a | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `recovery/strict-merge-gate-merge-readiness-20261022` | 2f96c6dadd4e | 2f96c6dadd4e | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `recovery/strip-tests-restore-20261030` | 8508f6f8388d | 8508f6f8388d | KEEP |  | donor; open P01 (#1317) |
| `rollout/velnor-3-provider` | 092fe1db7963 | 092fe1db7963 | DELETE-NOW |  | SAFE (retire-verify-foreign.md) |
| `sup019/fix-1264` | 2c54a59747a9 | 2c54a59747a9 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `sup020/fix-cli-1264` | aadbd7a40fc0 | aadbd7a40fc0 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `swarm/fanout-call_667899f3ad454fd1946b4556-2` | d864d067e4ad | d864d067e4ad | KEEP |  | donor; open P01 (#1317) |
| `swarm/fanout-call_cd50d53ea77340e08a7d675b-1` | 78855f5ad1b2 | 78855f5ad1b2 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `swarm/fanout-call_e6e20aee930345eb8831374f-0` | 50103fdf0932 | 50103fdf0932 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `swarm/sup009-sonar-source-config` | f0b2a8a307e5 | f0b2a8a307e5 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `test/cli-t09-t18-collision` | 0260cc4b0e92 | 0260cc4b0e92 | KEEP |  | group A, no verify file (pre-run) |
| `test/mv1-codex-placeholder-auth` | bc5498d9a1ad | 1e41b683952b | NEWLY-DELETABLE | yes | PR #1313 CLOSED, landed in #1321 51f2d808; ref == PR head |
| `test/provider-eligibility` | 327e45af61a3 | 327e45af61a3 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `w3/pr1281-land` | b2c0da82a816 | b2c0da82a816 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `w3/pr1281-merge` | e8d6bcca4638 | e8d6bcca4638 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a11b-account-hardening` | a935999a424e | a935999a424e | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-accounts-integrated` | ecfe7e96047a | ecfe7e96047a | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-availability` | 7a20e913e92e | 7a20e913e92e | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-availability-final` | ebfb26c9c396 | ebfb26c9c396 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-claude-model` | 4a3d4ee53242 | 4a3d4ee53242 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-grok-session` | 17aacafcfa08 | 17aacafcfa08 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-native-integrated` | 97c7a8ce644f | 97c7a8ce644f | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-oauth-blockers` | 3815766f08dc | 3815766f08dc | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-oauth-integrated` | 94d880efefe9 | 94d880efefe9 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-promotion` | a65c59a3bde2 | a65c59a3bde2 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-role-review` | 790a9e891005 | 790a9e891005 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-stable-pins` | b5d317b080e7 | b5d317b080e7 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-subscription-close` | 81cace3ddd3e | 81cace3ddd3e | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-subscription-verify` | 95feab027096 | 95feab027096 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-t02-blocker` | a31a8e7b8227 | a31a8e7b8227 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-t02-live-fixed` | f41625351c8b | f41625351c8b | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-t02-policy` | 8f4a4bfd31db | 8f4a4bfd31db | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-t02-policy-review` | 28f7e0751b71 | 28f7e0751b71 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-t03-promotion` | fbdb9667be33 | fbdb9667be33 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-t09-config` | d6ee3233e0f8 | d6ee3233e0f8 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-t09-live` | c700d207885f | c700d207885f | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-t09-verify` | f9d6621d2486 | f9d6621d2486 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-t13-auth` | 8c444a97ad71 | 8c444a97ad71 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a14-zai-final` | 36537f1dd06d | 36537f1dd06d | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a15-declared-auth-producer` | dd2008cbf9e7 | dd2008cbf9e7 | DELETE-NOW |  | SAFE (retire-verify-foreign.md) |
| `wt/a15-e2e-t01` | 655a00e2f128 | 655a00e2f128 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a15-final-clean` | 8c93c096dbad | 8c93c096dbad | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a15-final-review` | b667957821df | b667957821df | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/a15-supported-targets` | c78d1ab38556 | c78d1ab38556 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/live-sub` | 961ede2c24e1 | 961ede2c24e1 | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/live-t02t05` | a79fa139334e | a79fa139334e | DELETE-NOW |  | SAFE (retire-verify-2.md) |
| `wt/models-research` | 66e47d24c812 | 66e47d24c812 | DELETE-NOW |  | SAFE (retire-verify-2.md) |

### MOVED refs (SHA differs from v1)

| branch | v1 SHA | current SHA | final class | why it moved |
|---|---|---|---|---|
| `fix/p01-store-capture` | 4338566da9e3 | db33e84a7196 | KEEP | PR head advanced after v1; current equals the PR head |
| `fix/p02-exact-openrouter-money` | a9f3631b690d | 56e4d2586b40 | KEEP | PR head advanced after v1; current equals the PR head |
| `fix/p03-cancellation-recovery` | abf24394c4f1 | b6f14fa99dd7 | NEWLY-DELETABLE | PR head advanced after v1; current equals the PR head |
| `fix/p07-fleet-observation` | 3c39bbbf2a16 | 8ef2570e830e | NEWLY-DELETABLE | PR head advanced after v1; current equals the PR head |
| `fix/p08-role-manifest-refresh` | bf102c1856a1 | dd68ebbb8df9 | NEWLY-DELETABLE | PR head advanced after v1; current equals the PR head |
| `fix/p12-role-locks` | 7f321a34c2e5 | dc46590fa053 | NEWLY-DELETABLE | PR head advanced after v1; current equals the PR head |
| `fix/p15-final-image-paths` | e46889836c97 | 9485838605e2 | KEEP | PR head advanced after v1; current equals the PR head |
| `fix/p17-sandbox-write` | aadef4e155fd | ec9df1407ef1 | NEWLY-DELETABLE | PR head advanced after v1; current equals the PR head |
| `main` | b4ee3acd7e96 | f151478146fb | KEEP | #1324 merged into main (f151478146) |
| `test/mv1-codex-placeholder-auth` | 1e41b683952b | bc5498d9a1ad | NEWLY-DELETABLE | PR head advanced after v1; current equals the PR head |

## 2. Guarded commands (NOT EXECUTED)

Form: `git push origin --force-with-lease=refs/heads/<b>:<sha> --delete <b>`. The lease is the current SHA from section 1, so a ref that moved is refused. Never run with `--force`. Batches are at most 20 refs. Each line gives its justification after `#`.

### DELETE-NOW (130 refs, batches of 20)

#### Batch 1 of 7
```sh
- branch `backup/w3-local-usage` expected `f0aa365be70a94fdefc3fb6b3f4a5c2afa2db0aa` (SAFE (retire-verify-1.md))
- branch `chore/dirty-slice-1` expected `72401bf5e78c630f6fc2765e6391bb9ff7188635` (SAFE (retire-verify-1.md))
- branch `chore/dirty-slice-accounting-2026-11-14` expected `fe4fcb3a11d1d0441582e0455581622a81060834` (SAFE (retire-verify-1.md))
- branch `chore/restore-1283-remainder` expected `e519e06e4885b92cf630c476b1913ae98974a27e` (SAFE (retire-verify-1.md))
- branch `chore/triage-inventory-20261112` expected `e13360eb87518f70c45097366e384851f113ad8d` (SAFE (retire-verify-1.md))
- branch `docs/native-records-port-2026-11-15` expected `e3fdf0329c884b0f8dc55805719721ea170e62de` (SAFE (retire-verify-1.md))
- branch `docs/pr1228-replacement-20261026` expected `912fe737e08fb540be67573ce8b086b30c77ec0c` (SAFE (retire-verify-1.md))
- branch `docs/triage-inventory-postmerge-2026-11-15` expected `48d495d01b9573405db63e218902cbdb60b75ccb` (SAFE (retire-verify-1.md))
- branch `docs/triage-inventory-reclass-2026-11-13` expected `cf9acb7485e1800680fd00b62767af35e9713fe6` (SAFE (retire-verify-1.md))
- branch `feat/native-usage-accounts` expected `8b5b99cd2a28c3cd5ea809e95401cd17fc0b35b3` (SAFE (retire-verify-1.md))
- branch `feat/probe-j07-launcher-identity` expected `6214bd6dde907a9e3292f4c342f83ad1b9e93b38` (SAFE (retire-verify-1.md))
- branch `fix/1287-doc-a` expected `7f13b545154f05e89d63202c8d92c71576ff27a8` (SAFE (retire-verify-1.md))
- branch `fix/1287-doc-b` expected `bc5dbe048d5306a9bf31acf92a0eb8b8df1fa52f` (SAFE (retire-verify-1.md))
- branch `fix/cli-review-fixes` expected `b95cd9288837a55f52ffbdbb57336f2216218e34` (SAFE (retire-verify-1.md))
- branch `fix/core-plan-review` expected `b172385c87f7e063cc95397882f1e16c13be3cb8` (SAFE (retire-verify-1.md))
- branch `fix/port-1264-review-findings-2026-11-15` expected `ca2f48f6bd87bdc5c5e1f87160986698e3bcf338` (SAFE (retire-verify-1.md))
- branch `fix/pr1264-followups-w3` expected `05549f29aab9542675caf9f30617f1c00b343024` (SAFE (retire-verify-1.md))
- branch `fix/pr1264-review-followups` expected `d79ad9b29e1050c1f6df785a8e4352728b1eec91` (SAFE (retire-verify-1.md))
- branch `fix/roles-review-impl` expected `04471245d9b9f519690bb0a58353f9ee847d543a` (SAFE (retire-verify-1.md))
- branch `fix/roles-review-impl-v2` expected `3d2401c1b34bf7262c33cbc9695bda7a0637291a` (SAFE (retire-verify-1.md))
```

#### Batch 2 of 7
```sh
- branch `fix/typed-model-selection` expected `0aaf7a411ce0f35ebd9ca7e7722caed7101c65c6` (SAFE (retire-verify-1.md))
- branch `fix/usage-review-a` expected `f6a83f1167ecf6560bdfbcf9274d7afa2708a45a` (SAFE (retire-verify-1.md))
- branch `preserve/https-pr1279` expected `5302848e22ea128d3f8b3b58c18f0625f91c0f59` (SAFE (retire-verify-1.md))
- branch `preserve/jackin-work-scratch` expected `74b9025a0f5fdece24d5bc460a0f37006fe7c2be` (SAFE (retire-verify-1.md))
- branch `preserve/orch-1287-scratch` expected `4451a50b8adcf27dae21e9c61e1d754a0f20db4f` (SAFE (retire-verify-1.md))
- branch `preserve/p0.6-prototype-20261109` expected `63b2976a53df905df858e4d8a566839b79245545` (SAFE (retire-verify-1.md))
- branch `preserve/pi-durable-swarm-notes` expected `d901ab2393e88420a952d31891dde2e7938dfd17` (SAFE (retire-verify-1.md))
- branch `preserve/pr1264-merge` expected `bbe6b5bb70b306731181740389249ef07e4c656c` (SAFE (retire-verify-t3-body.md))
- branch `preserve/pull-1285-head` expected `71fa7744b4b2a1300897c97b1c118d87746b2243` (SAFE (retire-verify-1.md))
- branch `preserve/stash-fix-roles-review-impl` expected `befcdea355b2674b2dd1fb8034fb19f6ab845722` (SAFE (retire-verify-1.md))
- branch `preserve/stash-w8-port-feature-openshell-native` expected `f2da2d2159c02aea487d37c648906fd85f7891ac` (SAFE (retire-verify-t3-body.md))
- branch `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1aedd-52c9-7ec1-8c5f-9a41cce8d991-` expected `62b483ee4e7599ca929b584ef20d3566c2b8325e` (SAFE (retire-verify-subagent-v2.md))
- branch `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1aefc-957f-7911-b340-d5b4968bca6e-` expected `8c427113e53eb89070dd2bf8be4af144ccb098db` (SAFE (retire-verify-subagent-v2.md))
- branch `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af34-b536-70b0-95c7-45cd4e71d44c-` expected `e1dceb179bef0cca52986b0b1bdb49a51183adfb` (SAFE (retire-verify-subagent-v2.md))
- branch `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af57-e5e9-7443-ad58-f6e04b26b0ae-` expected `86fc1d637aee729775fa9a03bc44c0ac82cba7cc` (SAFE (retire-verify-subagent-v2.md))
- branch `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af6f-49a5-76c0-b265-0f5224993db2-` expected `1dbe3f65a1e2d6fc1c98d65b770ac91d1f1d5e52` (SAFE (retire-verify-subagent-v2.md))
- branch `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af7b-2b84-7a62-93f4-f7b4c789469c-` expected `16005c405aae301b62ed8c90bd52c7f3c5bdfbbf` (SAFE (retire-verify-subagent-v2.md))
- branch `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af8d-4412-7bd2-83b0-008313c1f23a-` expected `15c12d19e544add305bfa49d4f40de5e08fac536` (SAFE (retire-verify-subagent-v2.md))
- branch `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afaa-b762-7170-a3e6-9c9f07aa9035-` expected `8d1b528a680d37cd04c8ae3441adfd4b442fe8ff` (SAFE (retire-verify-subagent-v2.md))
- branch `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afcb-0959-7ce1-bc99-05490e535252-` expected `c5a98a0870c94ed7f8c5335945221e0c883d8044` (SAFE (retire-verify-subagent-v2.md))
```

#### Batch 3 of 7
```sh
- branch `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afdb-bcd8-7a43-8ac3-0cfcc4d54763-` expected `1190c7efac65e1cc5e3de7275acd9048e21345c5` (SAFE (retire-verify-subagent-v2.md))
- branch `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afe7-dbcc-7580-ad46-c84d6e0ab127-` expected `43fc09eeaf781c668d3399f8371fa0aebb54865e` (SAFE (retire-verify-subagent-v2.md))
- branch `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1aff2-7c8f-7c91-8944-99e86e1939a5-` expected `f424cce6020c81572e1caa2419cbe28d10b23dcd` (SAFE (retire-verify-subagent-v2.md))
- branch `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1b002-7a9c-70b2-9102-7a47e94b1726-` expected `5e1f8c2dbac1e54759593fd582140e11d48d9651` (SAFE (retire-verify-subagent-v2.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/0` expected `f6c2bff00d62b5d9bb7086a90bf4a76733077039` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/1` expected `c9a4ad50cad841b82c3aa1068d2bae525801bcf0` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/10` expected `1e928c0af82c5e0cfdb890b4031e7b78c4902424` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/11` expected `8d1e781bf921b6f2fb315c87000a5828380e305b` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/12` expected `10034b2a4d7763f9f7e16d761a1d232a4ab78717` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/13` expected `52a1aabe06e6c459233e96677082af97b4999591` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/14` expected `5170974948200a8d2bf2c562100a2265c78912d1` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/15` expected `7f1f0f92c40a927f01b4d63d547bdf302274071c` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/16` expected `7fc4466a93bb9c6d05f955a6032c3a3bac286046` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/19` expected `abe950fac0fa20637ff90ab6d1d1b605e86485bb` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/2` expected `d9c34f15fef0cbc23d739ad98f38632945ad3d83` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/20` expected `1460524035411829658eead415d50ad25027d99d` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/21` expected `e99b4a424c7bb036bfe7e8028da9aae7089b1186` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/22` expected `d7a22844cd196dcee8d03ffb8abc61c7e7d9aa12` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/23` expected `cd4fc9ee48b4eede9c78d2264a2321f0eaa6c784` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/24` expected `fe7c87c9e242e423b4789222e99ebf64712a1089` (SAFE (retire-verify-t3-body.md))
```

#### Batch 4 of 7
```sh
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/25` expected `22cd5514465453f4861e5a6effa491a41ba62ad9` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/26` expected `d7a1c0ef1020e97f3dce9b8af382f986d499fc45` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/27` expected `1ae7d77013d6f616e2d7be204b02b5ca1ef6b08b` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/28` expected `814b61d338d84db77c2f22ec98e40ff9d6196716` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/29` expected `4f4ea8bf5aab6d262a778e273f68edc74ea17d38` (SAFE (retire-verify-t3-body.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/3` expected `2c88c91d59285123db0874433c810047bbf1df13` (SAFE (retire-verify-2.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/30` expected `65f6a4008150674e55dce96a67697e1a8e24f9fa` (SAFE (retire-verify-2.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/4` expected `557429f3be14fee2c32782521dd8f132bb3ba058` (SAFE (retire-verify-2.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/5` expected `c8ad0d23823a307723a339a8f4956234fa769179` (SAFE (retire-verify-2.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/6` expected `8519884113da1d51233f5c0067dde45d93d1695c` (SAFE (retire-verify-2.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/7` expected `94d89669609617f03a942b3bcab4f446cec1f81f` (SAFE (retire-verify-2.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/8` expected `674353bbf9cea3682a348a8438033d3855d83974` (SAFE (retire-verify-2.md))
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/9` expected `4940d2902cc76f0f2080992c7f1318f040e9ea77` (SAFE (retire-verify-2.md))
- branch `preserve/t3/YjE5NTdmNGViMzdhNDM3ZTYwZjI1N2UwODdiYzZiMDA/0` expected `41a002522ee1050c57c0ace37f0cace7c7029a46` (SAFE (retire-verify-2.md))
- branch `preserve/t3/YjE5NTdmNGViMzdhNDM3ZTYwZjI1N2UwODdiYzZiMDA/1` expected `6cd95820272dcf05dcaa07bcb3ce6d27a17bff44` (SAFE (retire-verify-2.md))
- branch `preserve/t3/YjE5NTdmNGViMzdhNDM3ZTYwZjI1N2UwODdiYzZiMDA/2` expected `8bc03fa83b32301d03ac6e1039a41994b8d69cc2` (SAFE (retire-verify-2.md))
- branch `preserve/t3/YjE5NTdmNGViMzdhNDM3ZTYwZjI1N2UwODdiYzZiMDA/3` expected `bc17efa2ba8b931172564a32c126aa37a89b59cc` (SAFE (retire-verify-2.md))
- branch `preserve/w2-impl-strong-e2e-zai` expected `b8b0cda4990f8ecd7bac665699b539a5a94929fd` (SAFE (retire-verify-2.md))
- branch `preserve/w6-fix-strong-` expected `4aa4a01eabc3e836e87077684884e8a6391c6dde` (SAFE (retire-verify-2.md))
- branch `preserve/worktree-` expected `c09004bba05f4e4c97670f3de2ff944b11fcf1a2` (SAFE (retire-verify-2.md))
```

#### Batch 5 of 7
```sh
- branch `preserve/wt-` expected `3f3f6b826c5fce22c2623ae8b3475a7f8248e592` (SAFE (retire-verify-2.md))
- branch `recovery/pr1230-blob-auth-77b399f1-20261027` expected `77b399f1242685ea5b8916b8a4d82818eee92f25` (SAFE (retire-verify-2.md))
- branch `recovery/stash-2f8cc42b-20261023` expected `e5e6d1fd27efc3eee52b7e18574e4870c5a4a191` (SAFE (retire-verify-2.md))
- branch `recovery/stash-2fb075f8-20261023` expected `4d9540abf301f80b58f81f47d7691132ce48f36c` (SAFE (retire-verify-2.md))
- branch `recovery/stash-88f5643c-20261023` expected `ddf8861f45646f34b1ca087bd8ed6b9429f270c1` (SAFE (retire-verify-2.md))
- branch `recovery/stash-adoption-6a0d2f03-20261025` expected `ade401e79e3ea7f6107dbae7b4831c1f879b743c` (SAFE (retire-verify-2.md))
- branch `recovery/stash-be51ff69-20261023` expected `9cdf189b812caf2395048bf0d8dbccdadf1e83a0` (SAFE (retire-verify-2.md))
- branch `recovery/stash-d808d094-20261023` expected `00f98f32514aca8005c56dd2e44e3fc03c326b46` (SAFE (retire-verify-2.md))
- branch `recovery/strict-merge-gate-merge-readiness-20261022` expected `2f96c6dadd4e1974f89f5a5f7407c0f6ff221fae` (SAFE (retire-verify-2.md))
- branch `rollout/velnor-3-provider` expected `092fe1db796367479029d4e94379530f7f5965c0` (SAFE (retire-verify-foreign.md))
- branch `sup019/fix-1264` expected `2c54a59747a9faa53950d53696f866f3fec99b79` (SAFE (retire-verify-2.md))
- branch `sup020/fix-cli-1264` expected `aadbd7a40fc04c255a93c73200850f024ec4edcd` (SAFE (retire-verify-2.md))
- branch `swarm/fanout-call_cd50d53ea77340e08a7d675b-1` expected `78855f5ad1b2e2073a45dc74707acee0226eb2f3` (SAFE (retire-verify-2.md))
- branch `swarm/fanout-call_e6e20aee930345eb8831374f-0` expected `50103fdf09327aa41f40662b4e83ef7189484bf8` (SAFE (retire-verify-2.md))
- branch `swarm/sup009-sonar-source-config` expected `f0b2a8a307e5f9564035c0547ce434748946eea8` (SAFE (retire-verify-2.md))
- branch `test/provider-eligibility` expected `327e45af61a3872635b3dc400b7ab784733ca3cf` (SAFE (retire-verify-2.md))
- branch `w3/pr1281-land` expected `b2c0da82a816595428c2329a9e3f735491e63042` (SAFE (retire-verify-2.md))
- branch `w3/pr1281-merge` expected `e8d6bcca463850c3b90f2a5c1c05ef9e4185f810` (SAFE (retire-verify-2.md))
- branch `wt/a11b-account-hardening` expected `a935999a424e945d8f9ddd84cd06d49c1b9d73ae` (SAFE (retire-verify-2.md))
- branch `wt/a14-accounts-integrated` expected `ecfe7e96047a763ca6a1af771ca848137b5c4ffa` (SAFE (retire-verify-2.md))
```

#### Batch 6 of 7
```sh
- branch `wt/a14-availability` expected `7a20e913e92ee23d055ae10a3017f0c8e7599329` (SAFE (retire-verify-2.md))
- branch `wt/a14-availability-final` expected `ebfb26c9c3961ceba28e0cab84396c5530648b20` (SAFE (retire-verify-2.md))
- branch `wt/a14-claude-model` expected `4a3d4ee53242f0de9077bf83e7c0b43f01e4490b` (SAFE (retire-verify-2.md))
- branch `wt/a14-grok-session` expected `17aacafcfa08300200bea25a8d5562268efaaf04` (SAFE (retire-verify-2.md))
- branch `wt/a14-native-integrated` expected `97c7a8ce644fa84b962c9f22cf724c181efa9450` (SAFE (retire-verify-2.md))
- branch `wt/a14-oauth-blockers` expected `3815766f08dc9857748470fa4bc1b0c500286295` (SAFE (retire-verify-2.md))
- branch `wt/a14-oauth-integrated` expected `94d880efefe94cc9915c9369d15a8cb6a3507394` (SAFE (retire-verify-2.md))
- branch `wt/a14-promotion` expected `a65c59a3bde226d26163f97e9696b79990cb047f` (SAFE (retire-verify-2.md))
- branch `wt/a14-role-review` expected `790a9e89100553441506d1758da3ea459c38fe10` (SAFE (retire-verify-2.md))
- branch `wt/a14-stable-pins` expected `b5d317b080e78a271999462054f69066c0cecfba` (SAFE (retire-verify-2.md))
- branch `wt/a14-subscription-close` expected `81cace3ddd3e20d6c5176be171944bce9594f420` (SAFE (retire-verify-2.md))
- branch `wt/a14-subscription-verify` expected `95feab02709670f4fad639aef5966061dc699897` (SAFE (retire-verify-2.md))
- branch `wt/a14-t02-blocker` expected `a31a8e7b82279b3e239c0d7e4ecc1b730352fe73` (SAFE (retire-verify-2.md))
- branch `wt/a14-t02-live-fixed` expected `f41625351c8be58580039ecc273273ca0205c5ef` (SAFE (retire-verify-2.md))
- branch `wt/a14-t02-policy` expected `8f4a4bfd31db1f2fd98d5dfc8c23d73ab3d3743c` (SAFE (retire-verify-2.md))
- branch `wt/a14-t02-policy-review` expected `28f7e0751b71b5068103415bdfd53ff61c2c969b` (SAFE (retire-verify-2.md))
- branch `wt/a14-t03-promotion` expected `fbdb9667be33fc53a084e96faba7c457df5303ad` (SAFE (retire-verify-2.md))
- branch `wt/a14-t09-config` expected `d6ee3233e0f8181ac3bd211090d274328ffcd964` (SAFE (retire-verify-2.md))
- branch `wt/a14-t09-live` expected `c700d207885fc424637eedd1cd0a8e2670199374` (SAFE (retire-verify-2.md))
- branch `wt/a14-t09-verify` expected `f9d6621d2486614d7bbc9ec79728edad26128262` (SAFE (retire-verify-2.md))
```

#### Batch 7 of 7
```sh
- branch `wt/a14-t13-auth` expected `8c444a97ad719e0f087bd94c77ae1255e78d64ff` (SAFE (retire-verify-2.md))
- branch `wt/a14-zai-final` expected `36537f1dd06d2825ee969ad90c714b7f982f72f5` (SAFE (retire-verify-2.md))
- branch `wt/a15-declared-auth-producer` expected `dd2008cbf9e7d3e3af830905e588e3abe663c9dc` (SAFE (retire-verify-foreign.md))
- branch `wt/a15-e2e-t01` expected `655a00e2f128093ca970e804c01850e8bfd25a30` (SAFE (retire-verify-2.md))
- branch `wt/a15-final-clean` expected `8c93c096dbad63e2df638f5f1634f9dcfdcf06ff` (SAFE (retire-verify-2.md))
- branch `wt/a15-final-review` expected `b667957821df7008aeb6b6ed7be0f99902499e04` (SAFE (retire-verify-2.md))
- branch `wt/a15-supported-targets` expected `c78d1ab3855652e554a8e793fac56f747bf4dc70` (SAFE (retire-verify-2.md))
- branch `wt/live-sub` expected `961ede2c24e1c5435bdd2e5d93153ba63adc9c35` (SAFE (retire-verify-2.md))
- branch `wt/live-t02t05` expected `a79fa139334e0a8003e40e43be1d8a718735178c` (SAFE (retire-verify-2.md))
- branch `wt/models-research` expected `66e47d24c8123ae6eb1f247d4ae8736e9b52efde` (SAFE (retire-verify-2.md))
```

### NEWLY-DELETABLE (8 refs, one batch)

Operator note: `wt/p03`, `wt/p07`, `wt/mv1` track their own branch and are clean. `wt/p08`, `wt/p12` and `wt/p17` are clean and their HEAD equals the origin tip, but their configured upstream is `origin/main`, so their ahead counts (4, 8, 5) are not push signals. Deleting the remote refs does not touch worktrees. Each worktree is left with a gone upstream.

```sh
- branch `fix/p03-cancellation-recovery` expected `b6f14fa99dd7db20f1179fc52a13390345dd9f72` (PR #1314 CLOSED, landed in #1324 f151478146; ref == PR head)
- branch `fix/p07-fleet-observation` expected `8ef2570e830efd27ccf868e121824c63703329b2` (PR #1319 CLOSED, landed in #1324 f151478146; ref == PR head)
- branch `fix/p08-role-manifest-refresh` expected `dd68ebbb8df9a60c742e57602eceb747bacb3712` (PR #1309 CLOSED, landed in #1321 51f2d808; ref == PR head)
- branch `fix/p12-role-locks` expected `dc46590fa053587dd66282c445ca4e440fd510ab` (PR #1305 CLOSED, landed in #1321 51f2d808; ref == PR head)
- branch `fix/p17-sandbox-write` expected `ec9df1407ef10d7aa3df9d896f4b28ce68792fce` (PR #1308 CLOSED, landed in #1321 51f2d808; ref == PR head)
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/17` expected `9156f83145b39ff0f73d5a476a17847bc80c7c43` (donor for P08 only; PR #1309 CLOSED, landed in #1321 51f2d808; no verify file)
- branch `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/18` expected `1dd6b8e4ed5f7716662a74d184c9d382b9702ab9` (donor for P08 only; PR #1309 CLOSED, landed in #1321 51f2d808; no verify file)
- branch `test/mv1-codex-placeholder-auth` expected `bc5498d9a1ad89e2da89515360db066a7eea6325` (PR #1313 CLOSED, landed in #1321 51f2d808; ref == PR head)
```

## 3. Summary

- Counts: DELETE-NOW 130, NEWLY-DELETABLE 8, KEEP 20, total 158.
- MOVED: 10 refs (section 1). Four are KEEP: main moved to #1324's merge commit, and fix/p01, fix/p02 and fix/p15 moved to their open PR heads. Six are NEWLY-DELETABLE after the PR-head check (p03, p07, p08, p12, p17, mv1).
- Gone since v1 (not on origin): fix/p05, fix/p06, fix/p11, fix/p13, fix/p16, fix/p21 (new in v1), plus the v1 GONE rows.

### Judgment calls for the operator
1. **Retire-class refs from before this run.** DELETE-NOW includes refs like `feat/native-usage-accounts`, `fix/1287-doc-a` and `wt/*`. The v1 verify files marked them SAFE, and you said DELETE-NOW is "as in v1". Your KEEP rule covers "anything not created by this run", which would also cover them. I followed the v1 list. Confirm before running batches 1-7.
2. **Donor refs.** All six plan-pinned donors and every B-group donor stay KEEP. Each still serves an open P-task (P01 via #1317, P02/P11 via #1325, P15 via #1316). The only exception is `preserve/t3/…/17` and `/18`, which serve P08 alone. They have no verify file, since retire class was RETIRE-COVERED with verdict none. They are in NEWLY-DELETABLE (b). Drop them if you want a verdict first.
3. **fix/p16-provider-write-ownership and fix/p11-usage-cache-freshness** are gone from origin, so nothing is left to delete. P16 landed in #1324 (batch 2) and P11 is still open in #1325.
4. **Group A refs** (`fix/secrets-scan-squash-1264`, `recovery/branches-nightly-compat-20261029`, `test/cli-t09-t18-collision`) stay KEEP. Their PRs are merged or closed, but the refs predate this run and no verify file covers them.

### Dirty or unpushed worktrees (paths and counts)

69 worktrees in total. Counts are from `git -C <wt> status --porcelain` (dirty entries) and `git rev-list --count @{u}..HEAD` (ahead of the configured upstream). 45 have no upstream and are not flagged unless dirty.

**Dirty (8):**
- `/Users/donbeave/Projects/jackin-project/jackin` (main): 2 entries
- `/private/tmp/jackin-browser-review.pFc38B/copy` (detached): 83
- `/private/tmp/jackin-provider-cli-check` (detached): 10
- `/private/tmp/jackin-registry-review.AESk6T` (detached): 81
- `/private/tmp/jackin-usage-ux-preview` (detached): 12
- `/Users/donbeave/Projects/jackin-project/wt/p01` (fix/p01-store-capture, open PR #1317): 1
- `/Users/donbeave/Projects/jackin-project/wt/p03-residuals` (fix/p03-residuals, no upstream): 1
- `/Users/donbeave/Projects/jackin-project/wt/p15-final-image-paths` (fix/p15-final-image-paths, open PR #1316): 1

**Ahead of configured upstream (17; 214 commits in total):**
- `/Users/donbeave/Projects/jackin-project/jackin-native-usage` (feature/native-usage-accounts, own upstream): 12
- `/Users/donbeave/Projects/jackin-project/jackin-usage-ux` (feat/usage-accounts-ux, upstream main): 33
- `/Users/donbeave/Projects/jackin-project/jackin-ux-followup` (fix/usage-ux-followup, upstream main): 114
- `/Users/donbeave/Projects/jackin-project/jackin-ux-b1` (ux/b1): 3
- `/Users/donbeave/Projects/jackin-project/jackin-ux-b2` (ux/b2): 2
- `/Users/donbeave/Projects/jackin-project/wt/doc-gemini-usage` (upstream main): 2
- `/Users/donbeave/Projects/jackin-project/wt/p04` (branch gone on origin, upstream main): 7
- `/Users/donbeave/Projects/jackin-project/wt/p08` (upstream main; HEAD equals origin tip): 4
- `/Users/donbeave/Projects/jackin-project/wt/p09` (branch gone on origin, upstream main): 3
- `/Users/donbeave/Projects/jackin-project/wt/p10-build-note` (branch gone on origin, upstream main): 2
- `/Users/donbeave/Projects/jackin-project/wt/p12` (upstream main; HEAD equals origin tip): 8
- `/Users/donbeave/Projects/jackin-project/wt/p14-page-cycle` (branch gone on origin, upstream main): 4
- `/Users/donbeave/Projects/jackin-project/wt/p17` (upstream main; HEAD equals origin tip): 5
- `/Users/donbeave/Projects/jackin-project/wt/p18-picker-kinds` (branch gone on origin, upstream main): 4
- `/Users/donbeave/Projects/jackin-project/wt/p19` (branch gone on origin, upstream main): 6
- `/Users/donbeave/Projects/jackin-project/wt/p20-j15-failclosed` (branch gone on origin, upstream main): 2
- `/Users/donbeave/Projects/jackin-project/wt/p21` (branch gone on origin, upstream main): 3

Worktrees that check out a DELETE-NOW ref: none. Worktrees on a NEWLY-DELETABLE ref: `wt/p03`, `wt/p07`, `wt/p08`, `wt/p12`, `wt/p17`, `wt/mv1`. All are clean.

Counts are vs the configured upstream. For worktrees whose upstream is `origin/main`, the count reflects this run's branch commits, not unpushed work. Read these numbers as a guide, not as a push verdict.

## Final refresh, 2026-11-25 (supersedes the KEEP rows above for these refs)

Since the snapshot above, P01, P02/P11, P03/P16, P07, P08, P12, P15, P17 and MV-1 have all landed. The following refs from this run are now eligible. Each entry gives the branch, the expected SHA, and the reason.

- branch `fix/p02-exact-openrouter-money` expected `56e4d2586b40db9c3684382906f903740e0dc0b0` (PR #1325 landed in #1328, 55f2b36)
- branch `fix/p03-cancellation-recovery` expected `b6f14fa99dd7db20f1179fc52a13390345dd9f72` (PR #1314 landed in #1324, f151478)
- branch `fix/p07-fleet-observation` expected `8ef2570e830efd27ccf868e121824c63703329b2` (PR #1319 landed in #1324, f151478)
- branch `fix/p08-role-manifest-refresh` expected `dd68ebbb8df9a60c742e57602eceb747bacb3712` (PR #1309 landed in #1321, 51f2d80)
- branch `fix/p12-role-locks` expected `dc46590fa053587dd66282c445ca4e440fd510ab` (PR #1305 landed in #1321, 51f2d80)
- branch `fix/p17-sandbox-write` expected `ec9df1407ef10d7aa3df9d896f4b28ce68792fce` (PR #1308 landed in #1321, 51f2d80)
- branch `test/mv1-codex-placeholder-auth` expected `bc5498d9a1ad89e2da89515360db066a7eea6325` (PR #1313 landed in #1321, 51f2d80)
- branch `test/p08-manifest-gaps` expected `132e7bc911faba9caf5a96f807f2cc307bd710c2` (PR #1327 landed in #1328, 55f2b36)
- branch `fix/p11-usage-cache-freshness` expected `ce987bd0bbae3ff4c2e6cc05d7d53847a35b3118` (the original P11 commit, which #1328 landed in adapted form)
- branch `docs/branch-porting-record` expected `7bcb0ad322dbad681e27028688702fd47406ba81` (PR #1333 closed, superseded by #1335)

These become eligible only after their follow-up PR lands:

- branch `backup/p03-residuals-wip` expected `b27f50f4fed25da9ec6a50cc208d4824f3afab8d`, after #1326 lands.
- The donor refs in the table above: `recovery/strip-tests-restore-20261030` after #1331 (P01 residuals) lands. `preserve/session-boundaries-20261109`, `preserve/w1-fixes-20261109`, `codex/openshell-verified-checkpoint-20261108` and `codex/openshell-integration-checkpoint-20261108-122311` after #1326 (P03/P16 residuals) lands. `recovery/openrouter-exact-money-20261027` now, since P02 has landed and has no open follow-up. Before deleting any donor, confirm that the donor table's tasks and follow-ups have all landed.
- `fix/p01-store-capture`, `fix/p15-final-image-paths`, `fix/p04…`, `fix/p05…`, `fix/p06…`, `fix/p09…`, `fix/p10…`, `fix/p13…`, `fix/p14…`, `fix/p16…`, `fix/p18…`, `fix/p19…`, `fix/p20…`, `fix/p21…` and `integrate/batch-*` were already removed from origin when their PRs merged.
