# Branch porting run, 2026-11-24 to 2026-11-25

This record covers the run that recovered missing improvements from the repository's branches into `main` and prepared the retirement of obsolete refs. It holds every decision that the run made or that still waits on the operator. The code lives in the merged PRs. The open work lives in the open PRs below.

Baseline: `main` at `8d9b05927e266844b63c2a20893257061ab5a086`, with 148 branches.

## Files in this directory

| File | Content |
|---|---|
| `plan.md` | The audit and porting plan: tasks P01 to P21, the exact-SHA branch manifest, and the model policy. |
| `goal.md` | The goal statement that started the run. |
| `ledger.md` | The orchestrator's task ledger. |
| `manifest-2026-11-24-snapshot.tsv` | The branch inventory from 2026-11-24 (157 refs). It is an earlier snapshot and does not include the refs created after it. For current deletion decisions, use the final refresh section at the end of `deletion-manifest.md`. |
| `deletion-manifest.md` | The guarded deletion batches, which have not been run. |
| `verify/` | The verification reports behind every deletion verdict. |
| `pr-1287.md` | The analysis of the stale PR #1287. |
| `local-branch-audit.tsv` | The audit that shows every local branch's commits are on GitHub. |
| `handoffs/` | The state of each open PR when the run paused. |
| `landing-rule.md` | The merge and landing rules used in this run. |
| `gfast-on-macos.md` | How to run G-FAST reliably on macOS. |

## Landed on main

All of P01 to P21 are landed.

| Item | PR | Merge commit |
|---|---|---|
| P20 | #1299 | 3bb5151 |
| P14 | #1300 | 6a83f30 |
| P18 | #1301 | 85a921a |
| P10 | #1302 | 0130af3 |
| P04 | #1303 | b1920b5 |
| P19 | #1304 | 8388fac |
| P09 | #1306 | 2bc55e3 |
| P21 | #1307 | ffb36f1 |
| DESIGN.md Gemini line | #1311 | 552a8a0 |
| P12, P08, P17 and MV-1 (Codex placeholder auth test) | #1321, a batch PR | 51f2d80 |
| P13 | #1318 | b71b227 |
| P05 | #1315 | 9963ff1 |
| P06 | #1312 | f740b00 |
| P03 with P16, and P07 | #1324, a batch PR | f151478 |
| P15 | #1316 | 227e147 |
| P02 with P11, and the P08 regression tests | #1328, a batch PR | 55f2b36 |
| P01 | #1317 | f03c444 |
| Fixes from the cross-PR integration review | #1329 | 10a9e09 |

Gates and proofs:

- G-FULL passed on `227e147` with `G-FULL EXIT=0` and 2996 tests. It has not run again after #1328, #1317, and #1329.
- The live proofs ran against the pinned OpenShell 0.1.7 on a separate gateway. P17 `write_file_mode_and_atomicity` passes. P20 J15 passes. The contract suite passes Core 17/18, Transfer 8/8, and Terminal 6/6.
- The full e2e journey fails 7 steps. One cause is a stale ended-process Stop check (open decision 4). The other is the jackin-sentinel probe (open decision 5).

## Open work

Every branch below is pushed. The handoff for each is in `handoffs/`.

| PR | Item | Remaining steps |
|---|---|---|
| #1326 | The P03/P16 residuals: typed cancellable lookups, the second-signal report, rollback of owned providers, and a console notice | Run G-FAST on head `1d0ef275b`. Answer the 3 Codex threads. Land the PR. |
| #1322 | Deferred review comments from #1299 to #1321, the P08 in-flight race test, the #1311 review, and the P06 git exit-status test | Run G-FAST on `5558d84ed` without interruption. Answer any new Codex threads. Land the PR. |
| #1330 (draft) | `RemovalPlan` live-config guards | Run G-FAST on the final head: the last run was stopped after the clippy fixes. Confirm CI is green and answer every thread. Mark the PR ready. Land it. |
| #1331 (draft) | P01 residuals: bounded probes and the selected-row size cap | In this PR, first bound the `canonicalize` calls in session `source.rs` and `credentials.rs`. That is review item 3, which was deferred, and it must not stay deferred. Then get a green G-FAST and a final review that covers the whole diff. Land the PR. |

After the last merge, run G-FULL on `main`. Merge only through the tailrocks `merge.ts` script with `--accept-risk`, as `landing-rule.md` describes.

## Decisions waiting on the operator

1. Branch deletions: `deletion-manifest.md` lists 130 verified retire refs. It also lists the refs that are deletable now or will be: the 8 from the first refresh, and the final refresh section at the end of the file. That section covers every branch of this run whose PR has landed, the two branches pushed only to preserve local commits, and the donor refs together with the follow-up PRs they wait on. Each ref has an expected SHA. None has run.
2. PR #1287: the recommendation is to close it without merging. `pr-1287.md` gives the reasons.
3. The OMP cache revision from P01 no longer includes the store's change time, so an identical recreated store keeps its cached windows. This reverses a tested behavior and needs acknowledgement.
4. D09: OpenShell 0.1.7 accepts Stop and then Start on a sandbox whose main process has ended. Decide whether `Session::power`, the contract check, and the fake gateway accept that.
5. The jackin-sentinel probe (in a separate repository) reads `/proc/<pid>/exe` and `/dev/stdin`, which the guest refuses. Decide whether to fix it there.
6. #1326 moved the Termrock pin to `d5799531`, which carries Termrock #604 and #605. Confirm the paired revision.

## Local state when the run paused

Nothing exists only on the machine that ran this program.

- Every commit on every local branch is reachable on GitHub, from a branch or a PR ref (`refs/pull/<n>/head`). `local-branch-audit.tsv` lists each branch. The `ux/*` and usage follow-up branches are contained in the heads of merged PRs #1310, #1320, and #1323. To fetch a PR ref on another machine, run `git fetch origin refs/pull/<n>/head`.
- Four detached worktrees from other sessions held uncommitted work. That work is committed unreviewed on these branches. Do not merge them as they are.

  | Branch | Content |
  |---|---|
  | `preserve/provider-cli-check-20261125` | Edits to the `jackin` connections CLI. |
  | `preserve/usage-ux-preview-20261125` | A preview of the console connections board. |
  | `preserve/browser-review-20261125` | A copy of the native usage work used for a browser review. |
  | `preserve/registry-review-20261125` | A copy of the native usage work used for a registry review. |

- Two local-only branches were pushed: `backup/p03-residuals-wip` and `fix/p11-usage-cache-freshness`.
- This record holds the porting plan and the goal byte for byte, as `plan.md` and `goal.md`.
- No worktree has uncommitted changes. No repository has stashes or unpushed commits.
- This record leaves out the raw gate logs, as plan section 3 requires. It records their results.

## Process notes

- Models: the subagents were Sonnet 5.5 and Haiku 5.5, and the parent session ran on Opus 5.5 with the operator's approval. Fable was never used. Some early reviews used effort xhigh before the operator capped effort at high. None did after the cap.
- Several agents ran broad `pkill -f` early in the run and killed other agents' gates. That is now forbidden, as `landing-rule.md` and `gfast-on-macos.md` record.
