# GOAL: Finish Jackin/Velnor CI reliability and performance work

## A. Identity and pause status

- Handoff ID: `jackin-velnor-ci-reliability--20260921T220036Z--root--51368cd2`
- Created: `2026-09-21T22:00:36Z`; checkpoint published `2026-09-21T22:03Z`; audit updated `2026-09-22T00:00Z` (UTC minute recorded from coordinator session). Handoff/audit status: `PARTIAL`: known checkpoint is published and recoverable, but the original verbatim source remains only in a local attachment and some interrupted-worker/independent-clone resources are not yet durably mapped. See L--N; this is not an engineering completion claim.
- Original goal status: `PAUSED_BY_USER`. Handoff status is administrative only; it is not an engineering completion claim.
- Runtime status: goal controller for thread `01a0c53a-bb9c-74f1-b9bc-4c90b9e22327` was explicitly set to `paused` at `2026-09-21T21:59:18Z`. Active original workers were messaged to stop/preserve, then implementation workers were interrupted at safe boundaries. Remote GitHub jobs were not cancelled and must be re-observed.
- Coordinator: `/root` / Codex. Primary repo/remote: `jackin-project/jackin` / `git@github.com:jackin-project/jackin.git` (sanitized).
- Source checkout: `/Users/donbeave/Projects/tailrocks/jackin-project/jackin`, `docs/carried-failures-report` at `5a83caf09e6b241f08a7c4dbd4f119a5ef866c5b`, initially clean. Dedicated preservation worktree: `WT-JH`, branch `goal/handoff-jackin-velnor-20260921t220036z-51368cd2`, based on that SHA.
- Observed bases: Jackin main `df4671e4d9f2860e90a5c71d8d0bd85b23d23291`; Velnor main `45ef1ebef769c78f45315e11a798fdaafaef4c4e`.
- Resume authorization: explicit later user request only.

## B. Original goal and success contract

Recovered objective: finish the Jackin/Velnor CI reliability/performance work represented by Jackin #1053 and its carried-failures report; research and finalize Jackin #1044; compare/converge #1044, #1052, current main, Velnor runtime work, and all carried failures. Use correctness-first structural fixes, independent agents/reviews, complete CI history, committed/pushed scoped changes, and live verification. Do not claim six-nines or 120-second success without representative evidence.

Authoritative verbatim source is the paused goal's authorized local attachment `/Users/donbeave/.codex/attachments/c43af6bb-85f3-4b22-82e5-aac4c9643a34/pasted-text-1.txt`; it is nonportable and contains no credential material. A resumer must read it before acting. Material constraints: generic workflow architecture stays in Velnor; Jackin holds product semantics; generated workflows are only regenerated through supported pinned/published tooling; no competing handwritten planner/migration; all reviews/comments/checks must be refreshed at final heads; commits are DCO-signed and include `Co-authored-by: Codex <codex@openai.com>`; use `rtk`; preserve unrelated work; do not reset/clean/force-delete.

Post-resumption contract: integrate required related changes in dependency order, verify combined main, then clean only goal-exclusive obsolete local resources under the gates in E.6. Never blindly merge every experiment or delete shared/unknown resources.

## C. State at interruption

- Last engineering observation: Jackin #1044 (`6ff54ce5`) CI run `35657319778`, macOS FFI job `106524303464`, failed after successful `cargo-binstall`, `cargo:sccache`, and BoltFFI setup. Its rendered mold bootstrap assumed Linux and failed `unsupported mold architecture: arm64`. Product consumers were skipped, not rebuilt: fail-closed behavior worked. A Velnor generic platform-rendering worker was interrupted before any reported commit.
- Last intended action: inspect/preserve that worker worktree, implement a generic macOS-safe mold policy in Velnor, publish a runtime, then re-render/retest #1044.
- No primary-worktree merge/rebase/cherry-pick or dirty files were observed at pause. No deployment, release, policy-bypass, merge, remote cleanup, or local cleanup happened during this handoff.
- Remote jobs may still be running: Jackin CI/Main `35656264437`, Desktop `35656263744`; treat prior observations as stale.

## D. Requirement progress ledger

| ID | Requirement | Status | Evidence | Remaining / dependency |
|---|---|---|---|---|
| R1 | Product bugs (OpenRouter/render/OTLP) | VERIFIED_DONE | Jackin `7c4909fa`, `247c7948`, `c52e912b`; independent reviews recorded | re-observe main only |
| R2 | Accurate #1053 report | IMPLEMENTED_UNVERIFIED | Jackin #1067 `0515ad33` | reviewer blocked: use 41 not 40 units, 3 Swift jobs, no terminal attribution to in-progress Desktop, update #1052/pin facts |
| R3 | Apple migration | IN_PROGRESS | Jackin #1044 `6ff54ce5`, Velnor runtime `eed474c4` | generic macOS mold fix/runtime, selected #1065 source fixes, re-render/full proof |
| R4 | Canonical Mise tool facts | IMPLEMENTED_UNVERIFIED | Velnor #1054 `256c24bb`, local 2673 tests | final independent review/current checks |
| R5 | Receipt behavior | IMPLEMENTED_UNVERIFIED | Velnor #1055 `2a270947`, all-feature 2673 | current hosted/review |
| R6 | Native closure/selection | IMPLEMENTED_UNVERIFIED | Velnor #1056 `9f795b2e`, full 2675 | resolve review risks: arbitrary build-script reads; configured products losing scanner unknown/digest |
| R7 | Scheduled actions-read | IMPLEMENTED_UNVERIFIED | Velnor #1057 `70268cd5`; Policy `35658824796`, CI `35658828109` green | fresh independent review, merge/publish, then #1064 rebase |
| R8 | Composable phases | IMPLEMENTED_UNVERIFIED | Velnor #1058 `93cd45e9` safe activation split | review/current CI; capability merge then later separate protected published-runtime activation |
| R9 | Desktop evidence/concurrency | IMPLEMENTED_UNVERIFIED | Velnor #1050 `96b835d9` hosted green | resolve/review possible `merge_group.base_sha` omission |
| R10 | First-attempt collector | IMPLEMENTED_UNVERIFIED | Jackin #1064 `f1402583`, 343 tests | depends R7/runtime; rebase/review/current generated output |
| R11 | D3/no-work/performance | FAIL / UNPROVEN | #1053 completed PR Apple jobs 22m01/35m01; docs paths still schedule wide work | measure/preserve FAIL; no claim from green checks |
| R12 | Lifecycle/promotion | IN_PROGRESS | #1058 safe split and Velnor runtime publisher design | protected main publication/activation, no candidate authority |

## E. Preservation inventory

### E.1 Discovery/ownership scope

Audited known project roots `/Users/donbeave/Projects/tailrocks/jackin-project/jackin`, `/Users/donbeave/Projects/github/velnor`, `/private/tmp`, and `/tmp` only where session/worker records identified Velnor worktrees. Methods: `git worktree list --porcelain`, ref/status inspection, GitHub CLI APIs, and worker reports. No arbitrary private-directory scan, fetch/prune, cleanup, or Git index mutation was performed. The dedicated worktree/branch/PR auditor reports are summarized below; resumption must refresh every live ref and inspect unknown/detached resources before use.

### E.2 Local clone/worktree ledger

| ID | Exact path/type | Branch/HEAD | Ownership/activity | Future disposition |
|---|---|---|---|---|
| WT-J1 | `/Users/donbeave/Projects/tailrocks/jackin-project/jackin` primary | `docs/carried-failures-report` / `5a83caf` | GOAL_SHARED, initially clean | KEEP |
| WT-JH | `/private/tmp/jackin-goal-handoff-20260921t220036z-51368cd2` linked | handoff branch / checkpoint | GOAL_EXCLUSIVE, created for pause | KEEP until handoff PR durable |
| WT-J1044 | `/private/tmp/jackin-1044` linked | `migrate/apple-ci-generic` / `6f025e5a` | GOAL_EXCLUSIVE migration workspace | INTEGRATE_THEN_REMOVE |
| WT-J1044C | `/private/tmp/jackin-1044-current` detached | `c52e912b` | GOAL_EXCLUSIVE audit workspace | INTEGRATE_THEN_REMOVE |
| WT-J1044R | `/private/tmp/jackin-1044-review` detached | `6f025e5a` | GOAL_EXCLUSIVE review workspace | INTEGRATE_THEN_REMOVE |
| WT-J1064A | `/private/tmp/jackin-1064-actions` detached | `f1402583` | GOAL_EXCLUSIVE collector audit | INTEGRATE_THEN_REMOVE |
| WT-JD3 | `/private/tmp/jackin-d3-review.lzzUTz` detached | `4ba4a4cb` | GOAL_EXCLUSIVE D3 audit | INTEGRATE_THEN_REMOVE |
| WT-JOTLP | `/private/tmp/jackin-daemon-otlp` linked | `test/daemon-otlp-lifecycle` / `da1936f1` | GOAL_EXCLUSIVE historical fix workspace | REVIEW_SHARED; merged fix must be mapped before deletion |
| WT-JFIRST | `/private/tmp/jackin-first-attempt` linked | `ci/first-attempt-evidence` / `f1402583` | GOAL_EXCLUSIVE PR workspace | INTEGRATE_THEN_REMOVE |
| WT-JRENDER | `/private/tmp/jackin-render-epoch` linked | `fix/usage-render-epoch` / `bf8411f3` | GOAL_EXCLUSIVE historical fix workspace | REVIEW_SHARED; merged fix mapping required |
| WT-JREPORT | `/private/tmp/jackin-report-postmerge-correction` linked | `docs/report-live-results` / `0515ad33` | GOAL_EXCLUSIVE report PR workspace; worker interrupted | KEEP pending review correction |
| WT-VR50 | `/tmp/velnor-review-1050-current` detached | `96b835d9` | GOAL_EXCLUSIVE, no edits/process | INTEGRATE_THEN_REMOVE |
| WT-VR54 | `/private/tmp/velnor-1054-final` detached | `256c24bb` | GOAL_EXCLUSIVE, no edits/process | INTEGRATE_THEN_REMOVE |
| WT-VR56 | `/tmp/velnor-review-1056.bQ9KwB` detached | `9f795b2e` | GOAL_EXCLUSIVE, no edits/process | INTEGRATE_THEN_REMOVE |
| WT-VR58 | `/private/tmp/velnor-1058-safe` | `fix/composable-regen-phases-safe` / `3263ea5` | UNKNOWN; four pre-existing modified Velnor source files, exact diff not audited | BLOCKED |
| WT-VPH | `/private/tmp/velnor-regen-phases` | historical base `690b3935` | GOAL_EXCLUSIVE historical dirty phase implementation/testing | BLOCKED pending contents/ref preservation |
| WT-VMOLD | worker-created Velnor mold workspace | UNKNOWN | worker interrupted; path/head/diff not returned before pause | BLOCKED; first resumption audit |
| WT-VR1044 | Velnor candidate-lifecycle workspace | UNKNOWN | historical report only; may be shared | REVIEW_SHARED |

All listed worktrees require fresh `git -C <path> status --porcelain=v2`, `rev-parse`, `worktree list --porcelain -z`, common-dir, lock/prunable, upstream, stash, nested-repo, and process checks before any change/removal. No stash, ignored artifact, LFS object, or active process was positively identified as essential except the unknown dirty worktrees above.

### E.3 Branch/ref ledger

| ID | Repository / ref | Tip / PR | Purpose and future action |
|---|---|---|---|
| BR-J1044 | Jackin `migrate/apple-ci-generic` | `6ff54ce5`, #1044 | sole migration carrier; retain |
| BR-J1065 | Jackin `cicd/s2-repromote-80bc420d` | `909a9f54`, #1065 | donor only: fold selected source fixes, not generated tree |
| BR-J1067 | Jackin `docs/report-live-results` | `0515ad33`, #1067 | report correction; reviewer-blocked |
| BR-J1064 | Jackin `ci/first-attempt-evidence` | `f1402583`, #1064 | actions-read dependent |
| BR-V1050 | Velnor #1050 branch | `96b835d9` | merge-group/concurrency |
| BR-V1054 | Velnor #1054 branch | `256c24bb` | canonical MiseStepFacts |
| BR-V1055 | Velnor #1055 branch | `2a270947` | receipts |
| BR-V1056 | Velnor #1056 branch | `9f795b2e` | native input closure |
| BR-V1057 | Velnor `codex/schedule-actions-read` | `70268cd5` | actions-read |
| BR-V1058 | Velnor `fix/composable-regen-phases` | `93cd45e9` | safe phase capability |
| BR-V1044 | Velnor candidate-lifecycle PR branch | `60bb9326e6303c577bd15e952558f0dc02fd78f2` | lifecycle source repair; cannot merge until published runtime/activation regenerates pinned tree |
| BR-VMOLD | Velnor mold-fix branch | UNKNOWN | must locate/preserve first |

Remote tips were observed through GitHub before pause; no cached `refs/remotes/*` is proof of current server state. Branches without a known worktree/PR and all stashes/reflog-only commits remain a preservation gap recorded in K.

### E.4 Related PR ledger

| ID | PR / state at pause | Tested head / facts | Future action |
|---|---|---|---|
| PR-J1053 | Jackin #1053 MERGED `df4671e4` | report-only; PR Apple run `35651713880` green | retain history, inspect main outcomes |
| PR-J1052 | Jackin #1052 MERGED `edef2c1e` | schema-2 base | do not merge competing content |
| PR-J1044 | OPEN | `6ff54ce5`; failed job `106524303464` | carrier after generic prerequisites |
| PR-J1064 | OPEN | `f1402583` | R7/runtime dependent |
| PR-J1065 | OPEN | `909a9f54` | donor; close only after exact coverage proof |
| PR-J1067 | OPEN | `0515ad33`; review BLOCK | correct/review later |
| PR-V1050 | OPEN/CLEAN | `96b835d9`, hosted green | resolve/review |
| PR-V1054 | OPEN | `256c24bb`, prior checks green | review |
| PR-V1055 | OPEN | `2a270947` | CI/review |
| PR-V1056 | OPEN/UNSTABLE | `9f795b2e` | resolve risks/current checks |
| PR-V1057 | OPEN/CLEAN | `70268cd5`, CI/policy green | review/merge/publish |
| PR-V1058 | OPEN/BLOCKED | `93cd45e9`, safe split | review/current CI |
| PR-V1044 | OPEN | `60bb9326`; qualification `35660441660` passed, Policy `35660437293` failed stale candidate polling, CI `35660442225` pinned-render drift/cleanup | preserve; resolve through runtime publication/activation/promotion, never hand-edit generated YAML |

No human approvals were observed; automated Codex usage-limit comments are not reviews. A resumer must query all-state, paginated PR history/comments/reviews/threads and live heads before acting.

### E.5 Future-only integration order

`Velnor prerequisite branches -> Velnor main -> attested runtime product -> Jackin #1044/#1064/#1067 -> Jackin main`.

1. Locate/preserve/finish generic `BR-VMOLD`, then independently review it. Resolve R6 review risks. Fresh-review #1050/#1054/#1055/#1056/#1057/#1058 at exact current heads; rebase only when current main requires it. Target mutations are sequential; read-only audit may parallelize.
2. Publish and verify Velnor runtime products on protected main after each required producer merge. Never adopt candidate binaries or hand-edit consumer YAML.
3. In #1044, port #1065 selected source changes: raw BoltFFI canonical bytes/normalization removal, native watch/read contracts, retain `.swiftpm/**` exclusion; retain #1044's `26.0`, `desktop-release`, `.xcode-version`, and Renovate. Regenerate atomically through published runtime.
4. Prove #1044 macOS producer/artifact/digest/consumers, no-diff double render, exact selection/cache paths, Renovate, Policy/DCO/required checks, and measured warm/cold cohorts.
5. Rebase #1064 after #1057 runtime; correct #1067; independently review and land safe increments. Then re-observe Jackin main CI/Desktop evidence and original acceptance ledger.

### E.6 Future-only cleanup runbook

For every candidate above, first re-observe exact host/path/common-dir/HEAD/owner/process/status/stash/nested repo. Delete only after (1) every required change is verified in target or explicitly superseded/rejected with durable recovery, (2) all tracked/untracked/ignored/stash content is accounted for, (3) no agent/goal/stacked PR/recovery use remains, (4) post-integration validation and durable handoff/integration evidence exist outside candidate, and (5) exact resource identity still matches. Use normal non-force `git worktree remove`, then guarded branch deletion; no wildcards, prefix deletion, recursive removal, stash dropping, pruning, remote deletion, or unlocking. Re-enumerate and write a cleanup receipt. Shared/unknown resources remain intact.

## F. Findings and rejected approaches

- #1044 remains the only migration carrier; #1052 is base-only. #1065 is a partial donor, not another tree to merge.
- Current #1044 failure is a generic Velnor Linux-mold-on-macOS renderer defect. Do not add a Jackin special case. Its tool closure did succeed.
- #1058 safely separates capability from activation: inactive config keeps old runtime parseable; later activation requires a published protected SHA. Candidate runtime authority is rejected.
- #1053 PR Apple timings (22m01, 35m01) show correctness evidence only; performance/reuse/no-work remain FAIL/unproven.

## G. Verification evidence and known failures

- PASS: Velnor #1057 `70268cd5`; Policy `35658824796`, CI `35658828109` reported green.
- PASS: #1050 `96b835d9` hosted checks; focused rendered merge-group test passed. Risk: `base_sha_expression()` may omit merge-group base SHA.
- PASS: #1054 exact head `cargo test -p velnor-workflow`: 2673 passed/27 suites.
- PASS: #1055 `2a270947`: locked all-feature Velnor test 2673; generator check passed.
- PASS: #1056 `9f795b2e`: focused and full 2675 passed; see R6 risks.
- FAIL: #1044 `35657319778` / `106524303464`: `unsupported mold architecture: arm64` from Linux installer on macOS.
- FAIL: Velnor #1044 `60bb9326`: source qualification passed, but old checked-in policy expects a candidate artifact new qualification intentionally no longer publishes; CI has stale pinned-render/generated-tree failures. Required route is publish → protected activation/pin/tree promotion, not a policy bypass or generated-YAML edit.
- BLOCK: #1067 review factual errors described R2.
- STALE/NOT TERMINAL: Jackin main/desktop jobs above; no status inferred.

## H. Remaining work

**First task after explicit resumption:** read this record/goal/AGENTS, then inventory `WT-VMOLD` and any interrupted worker workspace without modifying it; preserve any unpushed diff/commit. Continue the generic macOS mold renderer fix only after confirming current live state. Required completion: scoped commit/push/PR, adversarial platform tests, current independent review, and runtime publication.

Then follow E.5; do not collapse requirements, call green reruns closure, or do cleanup before combined verification.

## I. Environment/recovery

Use `rtk`; Jackin root above. Velnor roots include `/Users/donbeave/Projects/github/velnor`, `/private/tmp`, and `/tmp`, all requiring fresh inspection. Generated output uses published/pinned `velnor-workflow`; macOS cohort is `macos-26`, Xcode `26.6`, deployment `26.0`. Required PR commits use `git commit -s` plus Codex co-author trailer. No secrets are recorded; use existing repository credential references only.

## J. Fresh-agent runbook

```text
/goal Read and resume docs/goal-handoffs/jackin-velnor-ci-reliability--20260921T220036Z--root--51368cd2.md
```

Read this document and the source attachment completely; read current instructions; create an exclusive worktree; compare all recorded heads/PRs/runtimes/jobs with live state; reconcile intervening changes without reset/force rewrite; resume at H; maintain scoped checkpoints; after original goal is genuinely complete, use E.5 and E.6 with fresh safety checks. The command resumes the original engineering goal and does not request another handoff.

## K. Blockers, omissions, and independent review

- Handoff is publishable but discovery coverage is incomplete until the four pause-only auditor reports are reconciled into this file. Exact all-worktree/ref/stash enumeration and `WT-VMOLD` location are currently `BLOCKED` by interrupted worker return/unfinished audit, not assumed absent.
- No implementation agent is authorized to continue. Known stopped workers: Velnor #1057 returned a safe committed/pushed head; mold/report workers were interrupted; review workers returned no edits or stopped.
- An independent handoff reviewer must read this file against audits before final `READY`; record findings/corrections here before commit.

## Receipt

Goal runtime pause verified; original work stopped/preserved to the degree documented; no merge or cleanup occurred. This unique handoff is on `goal/handoff-jackin-velnor-20260921t220036z-51368cd2`, draft [PR #1068](https://github.com/jackin-project/jackin/pull/1068), initial published commit `8a73c7039a8756e456a61e874472ef90f60ae7a1`. Future integration and cleanup are documented only and require explicit resumption.

## L. Audit repair: authoritative sources and requirement coverage

This section was added during the pause-only resumability audit. It supersedes the coarse R-ledger/H prose only where it is more precise. It does **not** resume the engineering goal. `G` is the original engineering contract; `H` is the pause/handoff contract.

### L.1 Source register

| Source ID | Type / location | Accessibility | Authority and use |
|---|---|---|---|
| S-G01 | Original `/goal` attachment: `/Users/donbeave/.codex/attachments/c43af6bb-85f3-4b22-82e5-aac4c9643a34/pasted-text-1.txt` | FULL to this session; nonportable | Original engineering goal, acceptance criteria, and definition of done. Its full verbatim text must be copied into a durable resumed-goal record before local attachment loss; this handoff preserves its operative atomic content below. |
| S-G02 | Later user amendment: research/finalize Jackin PR #1044 and compare/converge it with the goal | FULL conversation source; timestamp unavailable | Adds #1044-specific convergence/research obligation. |
| S-G03 | Later user amendment: autonomous ambiguity resolution; use independent agents | FULL conversation source | Binds continuing execution; no user clarification pause. |
| S-G04 | Later user amendment: frequent scoped commits/pushes; minimize branches | FULL conversation source | Binds resumed implementation and preservation. |
| S-H01 | User `IMMEDIATE GOAL PAUSE — PRESERVE STATE, WRITE HANDOFF, PUBLISH DRAFT PR` | FULL conversation source | Supersedes G execution temporarily; requires checkpoint, inventory, draft PR, future-only integration/cleanup. |
| S-H02 | User `VERIFY AND REPAIR THE PAUSED GOAL HANDOFF` | FULL conversation source | Current bounded audit authority; requires source matrix, executable tasks, fresh-reader review, corrected existing PR. |
| S-R01 | Jackin `AGENTS.md` supplied for this workspace | FULL | Repository conventions: correctness-first, DCO/co-author trailers, `rtk`, generated-source discipline, independent review. |
| S-E01 | Goal controller observation, thread `01a0c53a-bb9c-74f1-b9bc-4c90b9e22327` | FULL | Runtime state was `paused`; it is evidence of pause disposition, not remote-job cancellation. |
| S-E02 | Agent reports, local Git observations, PR/run records cited in D--K | SECONDARY_ONLY | State evidence only; never substitutes for user intent or live re-observation. |

S-H01 supersedes G's “continue until complete” only while pause is active. S-H02 authorizes audit/repair/preservation only. No source authorizes engineering implementation, merge, release, deployment, branch deletion, or worktree cleanup during this audit.

### L.2 Original contract — recoverable operative content

S-G01 is recoverable locally but not remote-portable. The following is a faithful consolidated reconstruction, not a claimed verbatim replacement: finish the complete Jackin/Velnor CI reliability and performance program represented by Jackin #1053, `CARRIED-FAILURES.md`, related plans/ledgers, and #1044; investigate architecture before patching; converge rather than duplicate #1044/#1052 migrations; preserve generic workflow capability in Velnor and Jackin product semantics in Jackin; use independent agents and independent verification; inspect complete live CI/PR history and current heads; land correct fixes under protection; and leave no unsupported six-nines/120-second/reliability claim. Required categories are selection/product closure, Apple migration/Renovate, Rust/tool/cache behavior, product defects, Velnor phase/promotion lifecycle, Desktop candidate/main coverage, first-attempt evidence, accurate report/ledger, measured performance, and final combined live proof. The full requirement matrix gives the executable reading of this reconstruction.

### L.3 Source-to-handoff requirement matrix

| Requirement ID | Authoritative source | Operative requirement | Exact HANDOFF section | Current state/evidence | Remaining task IDs / acceptance check | Coverage |
|---|---|---|---|---|---|---|
| G-001 | S-G01 §§1–4 | Correctness-first structural diagnosis; generic Velnor vs Jackin semantics; agents and independent verification | B, F, M | Architecture findings in F; no final combined proof | T-001 through T-014; each requires independent reviewer | COVERED |
| G-002 | S-G01 §3 | Refresh live branches, complete CI histories/attempts, PR reviews, runtime publication/adoption before change/merge | B, E, M.1 | Snapshot is historical; remote state stale | T-001; exact live heads/reviews/runs recorded before action | COVERED |
| G-003 | S-G01 §5; S-G02 | One converged #1044 path; no competing migration; preserve Apple, Renovate, provider/trust obligations | E.5, F, M.2 | #1044 carrier identified; #1052 base-only; generic mold failure | T-003/T-004/T-007; published runtime + atomic regen + required checks | COVERED |
| G-004 | S-G01 §6 | One canonical selection/read/product/expected-work contract; prove positive/adversarial cache hit/miss paths | D R6/R11, G, M.3 | #1056 unverified/current-review required; D3 proof absent | T-005/T-008; exact selection/adversarial tests and no-work proof | COVERED |
| G-005 | S-G01 §7 | Typed OpenRouter semantics; frozen render clock; deterministic OTLP lifecycle, exact exports | D R1, G, M.4 | Historical commits/reviews only; main re-observation needed | T-002; exact candidate/main evidence, regressions remain passing | COVERED |
| G-006 | S-G01 §8 | Composable Velnor regeneration phases and safe publish/promote/adopt identity | D R8/R12, F, M.5 | #1058 activation lifecycle blocked; dirty historical phase resource unknown | T-006; capability/publish/activation sequence and phase tests | COVERED |
| G-007 | S-G01 §9 | Measure actual cohorts/reuse/caches; retain FAIL until 120s cohorts truly pass | D R11, F, G, M.6 | Apple 22m01/35m01; no full cohort proof | T-010; measured cohort table and no unsupported claim | COVERED |
| G-008 | S-G01 §10 | Candidate/merge-group Desktop parity and lossless per-main evidence; no silent cancellation | D R9, F, M.7 | main-only Desktop/concurrency gap known | T-009; prospective-tree obligations and rapid-advance adversarial proof | COVERED |
| G-009 | S-G01 §11 | Tested first-attempt collector/rollup; accurate report/ledger and no statistical overclaim | D R2/R10, G, M.8 | #1064 depends #1057; #1067 factual corrections blocked | T-011/T-012; collector + report/ledger exact evidence | COVERED |
| G-010 | S-G01 §§12–13; S-G04 | Frequent signed/pushed scoped commits; protected integration; post-merge proof and final all-findings status | B, E.5–E.6, M.9 | Not complete; pause forbids landing now | T-013; independent review, current checks, merge policy, combined main proof | COVERED |
| H-001 | S-H01/S-H02 | Original goal stays `PAUSED_BY_USER`; audit only, no implementation/merge/cleanup | A, C, K, N | Goal controller observed paused | T-001 begins only after explicit resume | COVERED |
| H-002 | S-H01 | Preserve all goal work, worktrees/clones/refs/stashes/PRs and future cleanup gates | E, E.6, M.10 | Newly observed dirty detached resources need durable preservation audit | T-002/T-014; each resource mapped/recoverable before deletion | VAGUE before L; now COVERED but PARTIAL evidence |
| H-003 | S-H01/S-H02 | Existing unique handoff and existing draft PR only; publish corrected artifact; auto-merge off | A, J, N | #1068 draft existing | T-015 (this audit publication); remote head/file/PR body match | COVERED |
| H-004 | S-H02 | Fresh-agent resume must not need this chat; exact first action and task map | J, M, N | This audit adds tasks and source register | T-001 live reconciliation then T-002 recovery | COVERED |

Coverage count at this audit revision: 14 records; 12 `COVERED`, 1 `VAGUE before repair` corrected to covered with partial evidence, 1 source portability limitation (`S-G01`). Engineering status is intentionally separate: no G item is declared complete by this documentation audit.

## M. Executable remaining-work plan (future execution only)

All tasks below are prohibited until an explicit later resume. They replace the earlier one-paragraph H plan. “Discover command” means inspect current repository instructions/scripts rather than inventing a command.

| Task | Linked requirements | Starting evidence / concrete next action | Dependencies / safe parallelism | Validation and completion condition |
|---|---|---|---|---|
| T-001 | G-001,G-002,H-001,H-004 | Snapshot is stale. Retrieve this branch, read S-G01 if still accessible, `AGENTS.md`, this entire file; inspect live Jackin/Velnor heads, all PR states/comments/threads, all CI attempts/pages, runtime releases, and goal controller. | First; read-only; creates updated evidence ledger only. | Record exact current SHA/run/attempt/review links and changes from snapshot; do not trust old heads. |
| T-002 | G-005,H-002 | Preserve/reconcile interrupted workers and dirty detached worktrees before engineering. Known at-risk: `WT-J1044C` and `WT-J1064A` show goal-shaped tracked changes; `WT-VMOLD` location unknown; `WT-VPH` historical dirty location uncertain. Inspect status, diff, common-dir, ref, stash, process and ownership. | After T-001; coordinator owns Git writes. | Every relevant diff/commit/artifact has durable remote ref/authorized archive or explicit BLOCKED recovery path; then re-run product regression evidence on current main. |
| T-003 | G-003,G-004 | Diagnose generic Velnor macOS mold rendering from #1044 failure `35657319778/106524303464`; repair in Velnor, never Jackin special case. | T-001/T-002; independent implementation/reviewer agents. | Controlled Linux/macOS renderer tests, tool/bootstrap/product receipt tests, current PR review/checks, protected merge and published runtime. |
| T-004 | G-003 | Continue existing Jackin #1044 only after T-003 runtime exists. Rebase onto current main; port valid generic source intent, D3 contracts and Renovate/provider correctness; remove synthetic/manual workaround only after product transport works; regenerate atomically. | T-003 plus relevant Velnor prerequisites; conflicting generated-tree writers serialized. | Supported generator no-diff/double-render; macOS producer/consumer, cache hit/miss, required checks, trusted policy and Renovate proof. |
| T-005 | G-004 | Current Velnor #1054/#1055/#1056/#1057 require live review/rebase as needed. Resolve receipt invariant: selected producer must succeed, but out-of-plan/dynamically skipped producer must permit guarded consumer materialization; retain fail-closed expected-work. | May research/review in parallel; target merges sequentially. | Exact selection, rename/delete, unknown-read, failed/absent/duplicate/stale/wrong-platform receipt tests; current hosted checks and independent review. |
| T-006 | G-006 | Repair/rebase Velnor lifecycle #1044 and phase work #1058. Retain trust separation: disposable candidate qualification cannot authorize policy or runtime. Split capability -> protected main publication -> protected activation/pin/tree -> config phase adoption. | T-001/T-002; depends on live lifecycle state. | Bootstrap fixture repair, promotion atomicity, phase identity/order/no duplicates/failure/timing/lane override/cache/reuse tests; published attested runtime and atomic promotion proof. |
| T-007 | G-003,G-006 | Reconcile #1044 with #1052/#1065 after T-003–T-006. Use #1044 existing carrier, not a third migration; source-only donor selection and current base required. | Serialized with generated Jackin writes. | Full combined semantic diff/review; every #1044/#1052 obligation accounted for as integrated/superseded/rejected with evidence. |
| T-008 | G-004 | Establish Jackin D3 contracts/selection proof at final generated runtime. | After T-004/T-005; separate synthetic tests can parallelize. | AGENTS/instruction-only no-work plus policy checks; exact Bun/Swift affected/dependent sets; missing product cold path; no unrelated prerequisite fan-out. |
| T-009 | G-008 | Implement candidate/merge-group Desktop parity in generator source and authoritative policy; no global YAML hand edit. Preserve non-silent per-main verdicts. | May profile alongside T-010; final policy after generated path stable. | Rapid advancing PR/main, cancellation, duplicate same-SHA and obsolete-candidate tests; actual eligible required pre-merge/merge-group check and post-main evidence. |
| T-010 | G-007 | Measure cohorts after correctness work: event-to-terminal, queue, runtime/tools/cache, compilation/tests/aggregation/export; do not manufacture traffic. | After stable execution paths; read-only collection parallelizable. | Cohort table for docs, Rust, Swift, full/main, Desktop, release, warm/cold/dependency; 120s remains FAIL unless demonstrated. |
| T-011 | G-009 | Land/review Velnor #1057 permission support, publish runtime, then rebase/review Jackin #1064 collector. | #1057 before #1064; independent reviewer. | Pagination/rerun/missing/rename/cancel/duplicate tests; scheduled rollup is live and reconciles expected obligations. |
| T-012 | G-009 | Correct #1067 and all stale execution/report/ledger claims using live first-attempt evidence; retain historical records. | T-001 plus T-010/T-011 evidence. | Reviewer-approved factual diff; no stale link/count/unsupported six-nines/120s claim. |
| T-013 | G-001–G-010 | Integrate verified increments under actual policies; after each merge re-observe main/Desktop/publication; final combined run and status table. | Strictly sequential target mutations; reviews may parallelize. | Every original/new finding status with source/live proof; all required reviews/comments disposed; no open acceptance gate mislabeled closed. |
| T-014 | H-002,G-010 | Only after T-013, execute E.6 one resource at a time; update durable cleanup receipt. | Last; never shared/unknown resource. | All five E.6 gates, target coverage after squash/rebase checked, then guarded worktree removal/branch deletion and re-enumeration. |
| T-015 | H-003 | Publish this audit repair to existing #1068; refresh body/head/draft state. | Current bounded task only. | Remote branch, PR head, PR body, and canonical file agree; no auto-merge/queue. |

**First resumption action:** T-001. **First unfinished engineering action after reconciliation/preservation:** T-003 (generic Velnor macOS mold policy), unless T-002 discovers a different unpreserved goal artifact requiring durable checkpoint first.

## N. Audit record, fresh-reader test, and limitations

- Historical pause snapshot is retained in A–K. This audit adds observations rather than rewriting its interruption facts.
- Fresh-reader acceptance question: a resumer can identify the original outcome in B/L.2, pause constraints in A/L.1, recoverable resources in E, first action in M, and completion/integration/cleanup gates in M/E.5/E.6. The one nonportable original verbatim source is explicitly identified as S-G01; that prevents a `VERIFIED` outcome.
- Known audit correction: current `git worktree list` exposes Velnor worktrees `goal-handoff/generic-macos-swift-ci` (`8c904fbf`), `codex/1057-generated-repair` (`73c0071a`), `codex/1057-rebase-current` (`70268cd5`), `preserve/handoff-1402ca52/velnor-rust-scan` (`01e3ce81`), `WT-VR50`, and `velnor-wavepin2` (`4dec6b9e`). The pre-audit E.2 list is historical/incomplete; these must be added to the live ledger at T-001/T-002 before any removal. Known live dirty state includes `WT-J1044C` (migration-shaped input/generated changes) and `WT-J1064A` (collector/generator changes). Their exact durable preservation is an explicit T-002 gate, not a claim of clean state.
- Audit outcome: `PARTIAL`. Documentation now actionably maps the accessible source contract and known state, but exhaustive original-verbatim portability and every worker/clone/ref/stash recovery cannot be proven from this session snapshot. This is a handoff-quality limitation, not a claim that the engineering goal is blocked or complete.
- Independent audit reports requested: intent fidelity, state/remaining work, inventory chain, and fresh-reader review. Fresh-reader reviewer `/root/audit_fresh_reader` returned `PARTIAL` after reading the pre-repair handoff, S-G01, declared plans, and draft #1068. Its concrete findings were: absent portable source/register/matrix/task map; insufficient verification evidence; incomplete inventory chain; vague worker preservation and checkout recovery; and sparse PR body. L/M/N repair the source/register/matrix/task-map portions and record the remaining portability/inventory gaps. The other three requested audit reports were not received before publication; their absence is not approval and keeps outcome `PARTIAL`.
- Fresh-reader post-repair interpretation: original engineering outcome is B/L.2; pause limits are A/L.1; recovery starts with J then T-001/T-002; engineering begins T-003; integration is T-013 and only then cleanup T-014. Any resumer missing S-G01 must mark the verbatim-source portion unavailable, use this matrix plus current repository evidence, and never claim complete intent fidelity.
