# GOAL: Finish Jackin/Velnor CI reliability and performance work

## A. Identity and pause status

- Handoff ID: `jackin-velnor-ci-reliability--20260921T220036Z--root--51368cd2`
- Created/updated: `2026-09-21T22:00:36Z`; status: `BLOCKED` until the pending read-only branch/worktree/verification auditors and independent handoff reviewer return. The checkpoint itself is still being committed and published now; the blocker is completeness of the inventory, not loss of the preserved known state.
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

Goal runtime pause verified; original work stopped/preserved to the degree documented; no merge or cleanup occurred. This unique handoff is on `goal/handoff-jackin-velnor-20260921t220036z-51368cd2`; draft PR/published SHA are to be inserted after the required final audit/review. Future integration and cleanup are documented only and require explicit resumption.
