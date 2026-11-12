# Branch triage inventory — 2026-11-12

Per-repo inventory of every remote branch across the five tracked repositories.
Class: **valuable** = real feature/improvement/fix worth landing; **debris** = logs, scratch,
generated dumps, abandoned experiments, duplicates/superseded work. Debris is never merged.
Merged branches are deleted; every deletion carries its SHA proof.

Freshness proof at capture time (local main == origin/main, `rev-list --left-right --count main...origin/main` = 0 0):

| repo | origin/main |
|---|---|
| jackin-project/jackin | `d19cce66f548f8c557b765d8e5fd430477a064c4` |
| jackin-project/jackin-sentinel | `5fd9af372bffec8caa1d98b78a2f125eaaed00cf` |
| jackin-project/jackin-agent-smith | `a9b8c8aea` |
| jackin-project/jackin-the-architect | `d10672897a` |
| tailrocks/terminal-components-baseline | `b3034308cbc7c0b99f7dcc598ffdf3949c736124` |

## jackin-project/jackin — 29 branches (baseline 29 → 30 → 29)

| branch | class | evidence | action |
|---|---|---|---|
| `fix/pr1264-review-followups` | valuable | `9fdcce513`, 25/0 vs main, 72 files — review findings of #1264 | PR #1275 open |
| `codex/openshell-integration-checkpoint-20261108-122311` | valuable | `6965f5d1c`, 1068/5, 1 unique commit — owner-provider integration (15 files) | PR to open |
| `codex/openshell-verified-checkpoint-20261108` | valuable | `ce119545c`, 1052/5, 2 unique commits — attach readiness | PR to open |
| `codex/r7-proposal-preservation-20261109` | valuable | `e9e364091`, 1115/5, 3 unique commits, 49 `docs/openshell-v2` files absent from main | PR to open |
| `preserve/hostpath-display-audit-20261108` | valuable | `0f99ab72a`, 1118/5, 1 unique doc file | PR to open |
| `preserve/local-unique-20261109` | valuable | `ccb0606c5`, 1132/5, 12 unique commits, ~190 archive files | PR to open / preserve archive |
| `preserve/session-boundaries-20261109` | valuable | `16de1eedf`, 992/5, 1 unique commit over 272 files | PR to open (rebase) |
| `preserve/stash-p2-before-reconcile-20261109` | valuable | `8035a1460`, 1090/5, 3 commits — typed model selection absent from main `crates/` | PR to open (real code) |
| `preserve/uncommitted-sources-20261109` | valuable | `864510902`, 1122/5, 2 unique commits, 15 files absent from main | PR to open |
| `preserve/w1-fixes-20261109` | valuable | `78d8b5f02`, 983/5, 13 unique commits (~170 files) | PR to open (rebase) |
| `fix/roles-review-impl` | debris (superseded) | `04471245d`, 13/2 — the 2 commits' files are byte-identical to main | content landed via PR #1278 `d19cce66f`; keep until the operator confirms the branch tree difference |
| `recovery/branches-nightly-compat-20261029` | debris (superseded) | `9827b3adc`, 7/1 — tree byte-identical to `97b1b58f7` | **deleted** (SHA proof: 0 touched files differ from main; PR #1263 closed) |
| `fix/restore-docs-source-globs` | debris (superseded) | `1b77800e8`, 9/101 — glob file byte-identical to main, 2 stale test files remain | listed-debris |
| `recovery/pr1230-blob-auth-77b399f1-20261027` | debris (landed) | `77b399f12`, 72 unique commits | merged SHA `470fc5d6f` (PR #1230) |
| `recovery/openrouter-exact-money-20261027` | debris | `6ebbe1449`, 25/32, old layout | listed-debris (PR #1237 closed; merged as #1238 `f16eee70`) |
| `docs/pr1228-replacement-20261026` | debris | `912fe737e`, 3/35, 62 `openshell-v2` files absent from main | listed-debris (twin #1234 merged `3ffd6171`) |
| `codex/pr1264-attach-readiness-20261108` | debris | `e1fb608b1`, 0 unique commits | listed-debris |
| `codex/unpublished-openshell-checkpoint-20261109` | debris | `ac55cbf92`, 0 unique commits | listed-debris |
| `preserve/p0.6-prototype-20261109` | debris | `63b2976a5` — main has `rewrite/evidence/P0.6-panes.md` | listed-debris |
| `recovery/stash-2f8cc42b-20261023` | debris | `e5e6d1fd2`, 1/78, stash snapshot | listed-debris |
| `recovery/stash-2fb075f8-20261023` | debris | `4d9540abf`, 1/90, stash snapshot | listed-debris |
| `recovery/stash-88f5643c-20261023` | debris | `ddf8861f4`, 1/83, stash snapshot | listed-debris |
| `recovery/stash-adoption-6a0d2f03-20261025` | debris | `ade401e79`, 1/51, stash snapshot | listed-debris |
| `recovery/stash-be51ff69-20261023` | debris | `9cdf189b8`, 9/92 — carries a rustls-webpki bump (RUSTSEC-2026-0382/0383) vs main 0.103.16 | listed-debris — operator check of the advisory |
| `recovery/stash-d808d094-20261023` | debris | `00f98f325`, 1/92, stash snapshot | listed-debris |
| `recovery/strict-merge-gate-merge-readiness-20261022` | debris | `2f96c6dad` — no `merge-readiness` task in main | listed-debris |
| `recovery/strip-tests-restore-20261030` | debris | `8508f6f83`, 36 unique commits, rewritten by #1264 | listed-debris |
| `rollout/velnor-3-provider` | debris | `092fe1db7` — no merge base with main, 223 unique commits | listed-debris |
| `wt/login-import` | debris | `4abb579b3`, 0 unique commits | listed-debris |

Deletion rule applied: `git merge-base --is-ancestor origin/<branch> origin/main` (exit 0 = eligible).
All 28 non-main branches fail it (exit 1) because #1264 landed as a squash, so the one deletion above
was justified by content equality, not by ancestry. Every other branch is listed, never merged.

## jackin-project/jackin-sentinel — 3 branches

| branch | class | evidence | action |
|---|---|---|---|
| `main` | valuable (trunk) | `5fd9af372b`, merged PR #158 | keep |
| `fix/s158-postmerge-4` | valuable | `06441c415`, 5/0, 2 files — the four post-merge probe findings | PR #159 open (not merged) |
| `preserve/probe-identity-20261109` | valuable | `01f8ca58`, 51/1, 45 files +10865/-519, 49/49 unique — `PROBE_FILE`, `stream_regular_file`, parent-tick launcher identity absent from main | needs its own PR (operator cherry-pick, never a branch merge) |

## jackin-project/jackin-agent-smith — 5 branches

| branch | class | evidence | action |
|---|---|---|---|
| `main` | valuable (trunk) | `a9b8c8aea`, merged PR #217 `ff16c521c` | keep |
| `feature/openshell-native` | valuable | `c5718850` → `ff16c521c` | **merged** — branch deleted after merge |
| `recovery/…/velnor/fleet-92c3478…` | debris | `b9db5b14`, 1/14, generated velnor sync | listed-debris (PR #206 closed) |
| `recovery/…/velnor/fleet-b9c3156c…` | debris | `ece890fd`, 1/14, generated velnor sync | listed-debris (PR #207 merged `08cb1c2f`) |
| `recovery/…/jackin-agent-smith` | debris | `c58cfdca`, 1/3, empty WIP snapshot | listed-debris |

## jackin-project/jackin-the-architect — 10 branches

| branch | class | evidence | action |
|---|---|---|---|
| `main` | valuable (trunk) | `d10672897a`, merged PR #481 `063aeb69` | keep |
| `feature/openshell-native` | valuable | `060944ef8` → `063aeb69` | **merged** — branch deleted after merge |
| `preserve/betterleaks-1.10.0-20261109` | debris (superseded) | `cb5801f77`, 3 toolchain files — the betterleaks 1.10.0 bump landed in #481's pins | content landed via PR #481; listed-debris (not byte-identical to main) |
| `archive/cleanup-20261006/architect-mbx-b809284df675` | debris | `b809284d` | listed-debris |
| `archive/cleanup-20261006/architect-mbx-e390e399f2` | debris | `e390e399` | listed-debris |
| `archive/cleanup-20261006/architect-original-chain-e390e399f2a4` | debris | `e390e399` | listed-debris |
| `archive/cleanup-20261006/architect-reflog-commit-e0b97da3819e` | debris | `e0b97da3` | listed-debris |
| `archive/cleanup-20261006/architect-role-e0b97da381` | debris | `e0b97da3` | listed-debris |
| `archive/cleanup-20261006/architect-task/original-b809284` | debris | `b809284d` | listed-debris |
| `archive/cleanup-20261006/architect-task/original-e0b97da` | debris | `e0b97da3` | listed-debris |
| `archive/cleanup-20261006/architect-task/original-e390e39` | debris | `e390e399` | listed-debris |

## tailrocks/terminal-components-baseline — 7 branches

| branch | class | evidence | action |
|---|---|---|---|
| `main` | valuable (trunk) | `b3034308` | keep |
| `feature/openshell-native` | valuable | `7a38f9c67`, 8/0, 155 files — the console rewrite | PR #5 open (not merged) |
| `preserve/local-unique-20261109` | valuable | `6923fa36`, 8/1, 3 unique commits `ef0ebeac`, `c98c81b1`, `6923fa36` | needs PR — cherry-pick onto #5's branch after #5 lands |
| `test/pre-refactor-protection` | debris (mostly merged) | `fc624b74`, 666/2 — content landed via PR #2 squash `534e0ff5`; 4 files / +2003 still differ | listed-debris; only `44ec845c` is salvageable |
| `wip/tuiscotti-capture-only` | debris | `0b8b8bb0`, 649/288, 10023 files of `run-evidence/` and binary captures | listed-debris |
| `wip/tuiscotti-fixcheck` | debris | `8d352ebf`, 649/288, 2552 files of run evidence | listed-debris |
| `wip/tuiscotti-pin-fec38841` | debris | `1c980e88`, 1/3, superseded pin (main pins a newer tuiscotti revision) | listed-debris |

Already deleted (merged branches): baseline `design/interaction-polish` (PR #474 lineage),
`codex/preserve-jackin-receipts` (PR #7), `codex/tuiscotti-pb-baseline-rollout-20261107` (PR #6);
architect `rollout/agent-policy` (PR #474); agent-smith `recovery/…/fix/policy-ruleset-contexts`
(ancestor of main).
