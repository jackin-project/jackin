# RETIRE batch 1 verification (retire-verify-1)

Read-only verification of 65 branches the plan marked RETIRE. Current main: `origin/main` = `0130af3dd` (fetched; 3 commits newer than the plan's audited main `8d9b05927`: #1302 build note, P14 pagination fix, P20 E2E fix).

**Method.** (1) `origin/<branch>` checked against the manifest SHA: 65/65 match, none MOVED. (2) Unique commits, merge-base and changed paths computed per branch. (3) For every added line vs. the merge-base, exact-text presence in the `origin/main` tree (lever in scratchpad `lever/novelty.py`); absent lines grouped by path and inspected. (4) Symbol, test-name and design-doc checks on main for the non-trivial items. (5) Long rewrite-line checkpoints: tree/parent identity, ancestry and reachability checks, and one read-only Sonnet helper (name-level only; its git diff/show calls were blocked).

**Totals:** SAFE 29 | VALUE 0 | UNCERTAIN 36.

## Table

| branch | expected SHA | verdict | evidence (short) |
|---|---|---|---|
| `backup/w3-local-usage` | `f0aa365be70a94fdefc3fb6b3f4a5c2afa2db0aa` | SAFE | All non-CI novel code is on main (credential/OpenCode/OMP readers, wait.rs, renewal); OMP bounds = P01 |
| `chore/dirty-slice-1` | `72401bf5e78c630f6fc2765e6391bb9ff7188635` | SAFE | T08 supported flip rejected (D29, section 3); main keeps T08 unavailable; docs only otherwise |
| `chore/dirty-slice-accounting-2026-11-14` | `fe4fcb3a11d1d0441582e0455581622a81060834` | SAFE | Triage accounting doc only |
| `chore/restore-1283-remainder` | `e519e06e4885b92cf630c476b1913ae98974a27e` | SAFE | 0 novel lines |
| `chore/triage-inventory-20261112` | `e13360eb87518f70c45097366e384851f113ad8d` | SAFE | Triage inventory doc only |
| `docs/native-records-port-2026-11-15` | `e3fdf0329c884b0f8dc55805719721ea170e62de` | SAFE | 0 novel lines |
| `docs/pr1228-replacement-20261026` | `912fe737e08fb540be67573ce8b086b30c77ec0c` | SAFE | Stale openshell-v2 planning pack (docs and validator script) |
| `docs/triage-inventory-postmerge-2026-11-15` | `48d495d01b9573405db63e218902cbdb60b75ccb` | SAFE | Triage doc and decisions.tsv only |
| `docs/triage-inventory-reclass-2026-11-13` | `cf9acb7485e1800680fd00b62767af35e9713fe6` | SAFE | Triage doc and decisions.tsv only |
| `feat/native-usage-accounts` | `8b5b99cd2a28c3cd5ea809e95401cd17fc0b35b3` | SAFE | Novel lines only in generated CI and .github/velnor manifests; product 0 novel |
| `feat/probe-j07-launcher-identity` | `6214bd6dde907a9e3292f4c342f83ad1b9e93b38` | SAFE | Novel lines only in .audit logs and helper scripts |
| `fix/1287-doc-a` | `7f13b545154f05e89d63202c8d92c71576ff27a8` | SAFE | rewrite/triage debris-ratification docs only |
| `fix/1287-doc-b` | `bc5dbe048d5306a9bf31acf92a0eb8b8df1fa52f` | SAFE | rewrite/triage docs only (verified) |
| `fix/cli-review-fixes` | `b95cd9288837a55f52ffbdbb57336f2216218e34` | SAFE | Shared set: all product items on main (a); console junie/cancel (b) |
| `fix/core-plan-review` | `b172385c87f7e063cc95397882f1e16c13be3cb8` | SAFE | wait.rs: main has waited+probed budget (a) |
| `fix/port-1264-review-findings-2026-11-15` | `ca2f48f6bd87bdc5c5e1f87160986698e3bcf338` | SAFE | renewal and omp test names present on main (a) |
| `fix/pr1264-followups-w3` | `05549f29aab9542675caf9f30617f1c00b343024` | SAFE | Shared set (a)/(b); OMP/OpenCode bounds = P01 |
| `fix/pr1264-review-followups` | `d79ad9b29e1050c1f6df785a8e4352728b1eec91` | SAFE | Shared set (a)/(b); checkpoint test names present on main |
| `fix/roles-review-impl` | `04471245d9b9f519690bb0a58353f9ee847d543a` | SAFE | Shared set (a)/(b) |
| `fix/roles-review-impl-v2` | `3d2401c1b34bf7262c33cbc9695bda7a0637291a` | SAFE | 0 novel lines |
| `fix/typed-model-selection` | `0aaf7a411ce0f35ebd9ca7e7722caed7101c65c6` | SAFE | is_account_default and AccountDefaultUnsupported on main (a); API.md doc (b) |
| `fix/usage-review-a` | `f6a83f1167ecf6560bdfbcf9274d7afa2708a45a` | SAFE | Shared set (a)/(b) |
| `preserve/https-pr1279` | `5302848e22ea128d3f8b3b58c18f0625f91c0f59` | SAFE | 0 novel lines |
| `preserve/jackin-work-scratch` | `74b9025a0f5fdece24d5bc460a0f37006fe7c2be` | SAFE | Debris: cherry1.txt, main_patchids.txt |
| `preserve/orch-1287-scratch` | `4451a50b8adcf27dae21e9c61e1d754a0f20db4f` | SAFE | Triage docs and debris (sorted_rows.tsv, line_map.json) |
| `preserve/p0.6-prototype-20261109` | `63b2976a53df905df858e4d8a566839b79245545` | SAFE | Abandoned prototypes/p0.6 code, captures, logs (b); docs on main |
| `preserve/pi-durable-swarm-notes` | `d901ab2393e88420a952d31891dde2e7938dfd17` | SAFE | Supervisor inbox artifacts under .pi |
| `preserve/pr1264-merge` | `bbe6b5bb70b306731181740389249ef07e4c656c` | UNCERTAIN | Pre-review intermediate of #1264; reviewed final tree = squash; 632 intermediate-only lines absent on main, unreviewed |
| `preserve/pull-1285-head` | `71fa7744b4b2a1300897c97b1c118d87746b2243` | SAFE | Generated CI only |
| `preserve/stash-fix-roles-review-impl` | `befcdea355b2674b2dd1fb8034fb19f6ab845722` | SAFE | Shared set (a)/(b); roles_git test on main |
| `preserve/stash-w8-port-feature-openshell-native` | `f2da2d2159c02aea487d37c648906fd85f7891ac` | UNCERTAIN | 8-path worktree delta is (b); base snapshot 7bd1b7036 (1143 commits) unreviewed |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1aedd-52c9-7ec1-8c5f-9a41cce8d991-` | `62b483ee4e7599ca929b584ef20d3566c2b8325e` | UNCERTAIN | Rewrite-line checkpoint lineage; tip 01a1b002 name-level helper SAFE; body-level unreviewed |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1aefc-957f-7911-b340-d5b4968bca6e-` | `8c427113e53eb89070dd2bf8be4af144ccb098db` | UNCERTAIN | Rewrite-line checkpoint lineage; tip 01a1b002 name-level helper SAFE; body-level unreviewed |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af34-b536-70b0-95c7-45cd4e71d44c-` | `e1dceb179bef0cca52986b0b1bdb49a51183adfb` | UNCERTAIN | Rewrite-line checkpoint lineage; tip 01a1b002 name-level helper SAFE; body-level unreviewed |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af57-e5e9-7443-ad58-f6e04b26b0ae-` | `86fc1d637aee729775fa9a03bc44c0ac82cba7cc` | UNCERTAIN | Rewrite-line checkpoint lineage; tip 01a1b002 name-level helper SAFE; body-level unreviewed |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af6f-49a5-76c0-b265-0f5224993db2-` | `1dbe3f65a1e2d6fc1c98d65b770ac91d1f1d5e52` | UNCERTAIN | Rewrite-line checkpoint lineage; tip 01a1b002 name-level helper SAFE; body-level unreviewed |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af7b-2b84-7a62-93f4-f7b4c789469c-` | `16005c405aae301b62ed8c90bd52c7f3c5bdfbbf` | UNCERTAIN | Rewrite-line checkpoint lineage; tip 01a1b002 name-level helper SAFE; body-level unreviewed |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af8d-4412-7bd2-83b0-008313c1f23a-` | `15c12d19e544add305bfa49d4f40de5e08fac536` | UNCERTAIN | Rewrite-line checkpoint lineage; tip 01a1b002 name-level helper SAFE; body-level unreviewed |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afaa-b762-7170-a3e6-9c9f07aa9035-` | `8d1b528a680d37cd04c8ae3441adfd4b442fe8ff` | UNCERTAIN | Rewrite-line checkpoint lineage; tip 01a1b002 name-level helper SAFE; body-level unreviewed |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afcb-0959-7ce1-bc99-05490e535252-` | `c5a98a0870c94ed7f8c5335945221e0c883d8044` | UNCERTAIN | Rewrite-line checkpoint lineage; tip 01a1b002 name-level helper SAFE; body-level unreviewed |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afdb-bcd8-7a43-8ac3-0cfcc4d54763-` | `1190c7efac65e1cc5e3de7275acd9048e21345c5` | UNCERTAIN | Rewrite-line checkpoint lineage; tip 01a1b002 name-level helper SAFE; body-level unreviewed |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afe7-dbcc-7580-ad46-c84d6e0ab127-` | `43fc09eeaf781c668d3399f8371fa0aebb54865e` | UNCERTAIN | Rewrite-line checkpoint lineage; tip 01a1b002 name-level helper SAFE; body-level unreviewed |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1aff2-7c8f-7c91-8944-99e86e1939a5-` | `f424cce6020c81572e1caa2419cbe28d10b23dcd` | UNCERTAIN | Rewrite-line checkpoint lineage; tip 01a1b002 name-level helper SAFE; body-level unreviewed |
| `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1b002-7a9c-70b2-9102-7a47e94b1726-` | `5e1f8c2dbac1e54759593fd582140e11d48d9651` | UNCERTAIN | Rewrite-line checkpoint lineage; tip 01a1b002 name-level helper SAFE; body-level unreviewed |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/0` | `f6c2bff00d62b5d9bb7086a90bf4a76733077039` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/1` | `c9a4ad50cad841b82c3aa1068d2bae525801bcf0` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/10` | `1e928c0af82c5e0cfdb890b4031e7b78c4902424` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/11` | `8d1e781bf921b6f2fb315c87000a5828380e305b` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/12` | `10034b2a4d7763f9f7e16d761a1d232a4ab78717` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/13` | `52a1aabe06e6c459233e96677082af97b4999591` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/14` | `5170974948200a8d2bf2c562100a2265c78912d1` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/15` | `7f1f0f92c40a927f01b4d63d547bdf302274071c` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/16` | `7fc4466a93bb9c6d05f955a6032c3a3bac286046` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/19` | `abe950fac0fa20637ff90ab6d1d1b605e86485bb` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/2` | `d9c34f15fef0cbc23d739ad98f38632945ad3d83` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/20` | `1460524035411829658eead415d50ad25027d99d` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/21` | `e99b4a424c7bb036bfe7e8028da9aae7089b1186` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/22` | `d7a22844cd196dcee8d03ffb8abc61c7e7d9aa12` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/23` | `cd4fc9ee48b4eede9c78d2264a2321f0eaa6c784` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/24` | `fe7c87c9e242e423b4789222e99ebf64712a1089` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/25` | `22cd5514465453f4861e5a6effa491a41ba62ad9` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/26` | `d7a1c0ef1020e97f3dce9b8af382f986d499fc45` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/27` | `1ae7d77013d6f616e2d7be204b02b5ca1ef6b08b` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/28` | `814b61d338d84db77c2f22ec98e40ff9d6196716` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/29` | `4f4ea8bf5aab6d262a778e273f68edc74ea17d38` | UNCERTAIN | Checkpoint of rewrite-line snapshot (tree = parent); content unreviewed; reachable via donor refs |

## Detailed sections (UNCERTAIN; no VALUE items)

Plan rules applied to all: plan section 3 (keep #1296 native accounting, D29 eligibility, Termrock UI, velnor-gen CI); section 6 (no restoration of whole snapshots, no old scheduler, no old UI); section 5 P01–P21 for (c).

### A. Rewrite-line checkpoint lineage: `preserve/subagent-v2-…` (13 branches, 13 distinct trees)

Common finding: each ref is a separate ~2400-commit line (none is an ancestor of another, none is an ancestor of main). Each tree differs from main in about 800 paths. Tree-level novelty for `01a1b002`: 5502 added lines absent from main in 360 files. Absent-in-main: `.github/workflows` 1522, `.velnor` 50, `.github/actions` 44, `rewrite/blockers` 14 (all (b)); `crates/usage` 103, which is the Gemini Code Assist reader and test (`providers/gemini.rs`, 91 lines) removed by #1296 (`4a2b869c8`, its own body says "a Gemini execution account no longer calls Code Assist"; (b)). Present-path novelty is about 3760 lines in about 327 files; most of it is evolved code on main.

Helper result (Sonnet, read-only, name-level inventories; file `escalate-sub2-result.txt`): VERDICT SAFE for `01a1b002`, with (c) items: P02 (cent rounding in OpenRouter provider), P04 (lock/listing weaknesses in `jackin-core/src/fs.rs`), P16 (delete-on-failed-read in `accounts.rs`), and P20 (j15 fail-open; main commit `3bb5151fb` now fixes it). The helper states it did not compare function bodies, so it is not a body-level verdict.

Why UNCERTAIN: body-level verification for this tree and the other 12 trees is not done. Next step: `git diff -U0 <merge-base> origin/<ref>` per file, then compare each hunk against the current main file, for the 70 product files listed in `escalate-sub2-files.tsv`.

- `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1aedd-52c9-7ec1-8c5f-9a41cce8d991-` (62b483ee4e75): Rewrite-line checkpoint; tree-level profile in section header.
- `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1aefc-957f-7911-b340-d5b4968bca6e-` (8c427113e53e): Rewrite-line checkpoint; tree-level profile in section header.
- `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af34-b536-70b0-95c7-45cd4e71d44c-` (e1dceb179bef): Rewrite-line checkpoint; tree-level profile in section header.
- `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af57-e5e9-7443-ad58-f6e04b26b0ae-` (86fc1d637aee): Rewrite-line checkpoint; tree-level profile in section header.
- `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af6f-49a5-76c0-b265-0f5224993db2-` (1dbe3f65a1e2): Rewrite-line checkpoint; tree-level profile in section header.
- `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af7b-2b84-7a62-93f4-f7b4c789469c-` (16005c405aae): Rewrite-line checkpoint; tree-level profile in section header.
- `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1af8d-4412-7bd2-83b0-008313c1f23a-` (15c12d19e544): Rewrite-line checkpoint; tree-level profile in section header.
- `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afaa-b762-7170-a3e6-9c9f07aa9035-` (8d1b528a680d): Rewrite-line checkpoint; tree-level profile in section header.
- `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afcb-0959-7ce1-bc99-05490e535252-` (c5a98a0870c9): Rewrite-line checkpoint; tree-level profile in section header.
- `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afdb-bcd8-7a43-8ac3-0cfcc4d54763-` (1190c7efac65): Rewrite-line checkpoint; tree-level profile in section header.
- `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1afe7-dbcc-7580-ad46-c84d6e0ab127-` (43fc09eeaf78): Rewrite-line checkpoint; tree-level profile in section header.
- `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1aff2-7c8f-7c91-8944-99e86e1939a5-` (f424cce6020c): Rewrite-line checkpoint; tree-level profile in section header.
- `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1b002-7a9c-70b2-9102-7a47e94b1726-` (5e1f8c2dbac1): Rewrite-line checkpoint; tree-level profile in section header.

### B. Checkpoint tips of rewrite snapshot family `preserve/t3/…` (21 branches)

Tip-tree equality verified: t3-0, 1, 2, 10–16, 19–25 have trees identical to their parent. Parent snapshots: `5076ca2a0` (t3-0), `4691c0089` (t3-1, 2, 10–16), `573c3a09e` (t3-19–25), `7bf7fe261` (t3-26–28), `589b3b6f4` (t3-29). t3-26–29 differ from their parent only by `rewrite/GOAL-ALL-AGENTS.md` (142 lines, obsolete prose, (b)).

Reachability (no-loss) checked: `5076ca2a0`, `4691c0089` and `573c3a09e` are ancestors of `origin/codex/openshell-verified-checkpoint-20261108` (plan donor, kept by section 1). `7bf7fe261` and `589b3b6f4` are ancestors of non-batch refs (`origin/sup020/fix-cli-1264`, `origin/swarm/...`), not evaluated further. Deleting the t3 refs therefore loses no commits; the content question is still open.

Content profile for `4691c0089` and `573c3a09e` (same profile as the lineage in A): absent-in-main = CI, `.velnor`, `rewrite/blockers` (b) and `crates/usage` (Gemini removal (b); OpenCode fixtures replaced by inline helpers (a)). About 3600 present-path novel lines are not reviewed.

Why UNCERTAIN: same as A. Next step: body-level hunk comparison of `4691c0089` and `573c3a09e`, which account for 16 of the 21 refs.

Branches: `t3-0`, `t3-1`, `t3-10`, `t3-11`, `t3-12`, `t3-13`, `t3-14`, `t3-15`, `t3-16`, `t3-19`, `t3-2`, `t3-20`, `t3-21`, `t3-22`, `t3-23`, `t3-24`, `t3-25`, `t3-26`, `t3-27`, `t3-28`, `t3-29`

### C. `preserve/stash-w8-port-feature-openshell-native` (`f2da2d215`)

Verified: stash index parent `e3781270a` is empty against HEAD `7bd1b7036` (`git diff --stat` 0 lines). The worktree delta is 8 paths: `crates/jackin-core/src/agent.rs` (blocker wording, (b)), `mise.lock` and `mise.toml` (pins, (b)), `rewrite/DESIGN.md`, `rewrite/blockers/T06/T07/T08`, `rewrite/decisions.tsv` (docs, (b)). Gap: the HEAD parent `7bd1b7036` is a 1143-commit rewrite-line snapshot (5172 novel lines in 341 files). It is not reachable from `codex/openshell-verified-checkpoint`. Next step: decide whether `7bd1b7036` belongs to the lineage in A.

### D. `preserve/pr1264-merge` (`bbe6b5bb7`)

Verified: reviewed final `f0021a0` has the same tree as squash `f683619b5`. `git diff f683619b5 bbe6b5bb7`: 1175 intermediate-only added lines, 632 absent from main, in 99 files. Largest groups: kimi, openrouter, minimax, zai, opencode provider code (money and quota paths; exact money is P02, the rest look superseded by the later provider rewrite), `crates/jackin-session/examples/*.rs` (59 lines), `crates/jackin-core/src/fs.rs` (25 absent lines; P04 area), `dispatch.rs`, `load.rs`, `multi_session.rs`. Why UNCERTAIN: not reviewed per line. A dropped-by-review item would be a real gap, so the plan's "reviewed final = squash" argument does not by itself prove the 632 lines are covered.

## Side findings (not part of the verdicts)

- `rewrite/DESIGN.md` line 343 on main still says the Gemini provider reads usage through `loadCodeAssist`/`retrieveUserQuota`. Main's `dispatch.rs` returns `unsupported` for Gemini, as #1296 intended. This is a stale doc line on main.
- `main` moved 3 commits after the plan's audited SHA (#1302, P14, P20). These are plan items; none changes a batch verdict.
- Escalations used: 1 (Sonnet 5.5, high, read-only; result file `escalate-sub2-result.txt`).
