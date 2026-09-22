# Carried failures — details and recommendations

Jackin CI/CD goal, refreshed 2026-09-23. Current state is recorded first;
older measurements and recommendations remain as historical evidence.
Evidence record: [EXECUTION.md](EXECUTION.md).

## Current authoritative carried state — 2026-09-23

Parent HEAD before this docs correction: `b5011fd`; current PR #1089 head is
`b5011fdffa8d5390c77b7833c5555a757ec2de4c`, based on `main`
`55e21f05`. Repairs are integrated, but the overall goal is not complete.
Fresh PR CI is pending.

| Area | Evidence | Verdict |
|---|---|---|
| Collector | Evidence rollup schema `4` consumes the durable push-head `push-head.json` artifact schema `1`; predecessor-boundary, chain, provenance, remote-tip, and wrong-workflow checks pass. Post-rebase xtask full serialized `402`, evidence-focused `30`, xtask clippy, generator `--plain --check`, actionlint, fmt, and diff pass. | Structurally repaired and fail-closed. **Not production-qualified**: no default-branch `ci-evidence` ledger run/artifact exists. |
| Performance audit | Post-rebase xtask full serialized `402` and evidence-focused `30` pass; failure classification excludes legitimate skipped/neutral/success jobs; warm evidence had 40/40 report producers valid and zero missing/fallback/parse failures. | Semantics fixed. No 120-second or cross-workflow reuse claim. |
| Provider retries | Post-rebase `jackin-usage` lib `588` passed and usage clippy passed; typed status/retry-after handling covers OpenRouter, Claude, and Codex. | Local coverage PASS. Real-provider behavior remains unverified; no live credentials or provider fixture run is claimed. |
| FFI | Post-rebase FFI `16` tests passed and FFI clippy passed. | Local PASS. |
| Desktop | Generated candidate and merge-cadence trigger/parity workflows were rerun after rebase. | Advisory only. Desktop is not a required ruleset context and has no claimed live pre-merge proof. |

Schema boundary: evidence/rollup schema `4` and push-head artifact schema `1`
are separate contracts. Schema `1` is the durable `ci-push-head-ledger`
artifact input; it is not an evidence schema `4` artifact and neither is
interchangeable with the other.

### Upstream and PR status

- Velnor [#1076](https://github.com/tailrocks/velnor/pull/1076) is at
  `36f0d4f4981e125dacec96eaf82f24369d38251b`. [Policy run
  35767882457](https://github.com/tailrocks/velnor/actions/runs/35767882457)
  completed **FAILURE**: `Acquire candidate generator product` found no
  candidate product within 15 minutes because no same-head `ci-pr` workflow run
  existed. DCO passed. Rawls recorded the head as 9 commits behind Velnor main
  `d4443aa` and returned **NO-GO** for preview/release and Apple/fork gaps. The
  live API reports current Velnor main `52dc35b`; no current behind-count claim
  is made. GitHub reports `CONFLICTING`/`DIRTY`. The repair is neither live-proven nor
  published/adopted by Jackin. The Jackin pin remains unchanged.
- Jackin [#1089](https://github.com/jackin-project/jackin/pull/1089) is the
  current carrier at head
  `b5011fdffa8d5390c77b7833c5555a757ec2de4c`, based on `main`
  `55e21f05bdee37a1424464ce69b86fc5e550307a`. GitHub reports the PR as
  mergeable but `BLOCKED`; DCO and Policy pass, while `ci-required` is absent
  because the Apple FFI check remains queued. Desktop merge cadence is advisory
  and in progress. No merge claim is made. PR #1083 is not the current carrier
  and was not reused.

### Carried blockers

1. Complete the pending CI for Jackin #1089; do not claim merge while its
   queued checks remain.
2. Merge and publish Velnor #1076, then adopt/regenerate its runtime in Jackin.
3. Obtain a default-branch ledger run/artifact before claiming collector
   qualification.
4. Close empty-selection, release-tool, result-provenance, merge-group, and
   Desktop obligation gaps.
5. Keep the 120-second and cross-workflow reuse rows FAIL/UNPROVEN until
   representative live measurements pass.

No completion, six-nines, full-inventory, or reuse claim is made.

## Historical snapshot — 2026-09-22 (superseded)

The mandatory evidence is preserved exactly here so a rerun cannot overwrite a
first attempt:

- [Run 35521080097 attempt 1 / job 106105226160](https://github.com/jackin-project/jackin/actions/runs/35521080097/job/106105226160), main `fce94cea8a15de0c2db3bb4ff880d741baf5c00a`: diagnostics failed `conformance_partial_success_is_not_retried` at `mod.rs:51`, left `7` versus right `1`; `108/122` ran, `107` passed, `1` failed, `1` skipped, and `14` were not run.
- [Run 35521080097 attempt 2 / job 106150119934](https://github.com/jackin-project/jackin/actions/runs/35521080097/job/106150119934): same diagnostics path succeeded at `21:11:07–21:12:58Z`. This is a separate successful attempt, not a closure of the first-attempt failure.
- [Run 35515575859 / job 106090835001](https://github.com/jackin-project/jackin/actions/runs/35515575859/job/106090835001), head `0163d1b7c654753f23d9ee7334866569c24615d0`: Desktop was cancelled after Mise found and installed `26` tools; `Run desktop-merge` lasted `34m57s`, with source-fallback installs for cargo-audit, cargo-dylint, codebook-lsp, and dylint-link.
- [Current main run 35722770593 / job 106729665225](https://github.com/jackin-project/jackin/actions/runs/35722770593/job/106729665225), head `de046d3345bb2ec44f9749655cd29164b6e4e4c9`: `jackin-usage` render failed because the output said `expires in 29d` while the test required `expires in 30d`; `148/577` ran, `147` passed, `1` failed, and `429` did not run.
- PR [#1079](https://github.com/jackin-project/jackin/pull/1079), commit `7090d8c45c6cd72644dc66f0732a6b50c7167403`, has green candidate checks in [run 35738453565](https://github.com/jackin-project/jackin/actions/runs/35738453565). This is not completion evidence.

### Carried blockers

1. Velnor phase identity can be lost during regeneration/preparation; the
   generic phase fix is not yet published and adopted by Jackin.
2. Empty selection lacks the upstream proof contract required for a fail-closed
   aggregate.
3. Release auto-install is disabled by #1079, but exact locked release tools
   are not yet provisioned; release coverage remains open.
4. Required result artifacts do not yet carry the complete candidate/base/plan/
   generator/phase/lane provenance contract.
5. `merge_group` is absent from the generated admission path; no merge-group
   run proves PR-to-queue parity. Desktop and release are also not full PR
   obligations.
6. Measured PR/main/Desktop classes exceed `120s`; cache pressure, MBX churn,
   repeated bootstrap, and unproven cross-workflow native reuse remain.

The acceptance matrix is maintained in [EXECUTION.md](EXECUTION.md). No
completion, six-nines, full-inventory, or reuse claim is made.

---

## Post-#1053 live result — correctness green; performance and no-work remain FAIL

PR #1053 merged as [`df4671e4`](https://github.com/jackin-project/jackin/commit/df4671e4d9f2860e90a5c71d8d0bd85b23d23291).
The report changed the tracked generator state sidecar: the scan digest in
`.github/ci/.github-actions-generator-state` moved from `7229bed326311884` to
`bbbfa7f57b91cb3b`. That generated-state change is a global input. The live
Control / Planning job therefore selected all 40 units with the explicit
fail-closed fallback. This is correct safety behavior; it is not evidence that
the no-work path works for documentation changes.

The merged candidate's Apple evidence is:

| Evidence | Live result | Verdict |
|---|---|---|
| [CI / PR run 35651713880](https://github.com/jackin-project/jackin/actions/runs/35651713880), [Swift job 106505596038](https://github.com/jackin-project/jackin/actions/runs/35651713880/job/106505596038) | 22m01s; 78 Swift tests passed; native build/code-generation work repeated, including a fresh `desktop-release` compile and 380 compiler-output lines | Correctness green; 120s and validated reuse FAIL |
| [post-merge Desktop run 35656263744](https://github.com/jackin-project/jackin/actions/runs/35656263744), [Desktop job 106520585495](https://github.com/jackin-project/jackin/actions/runs/35656263744/job/106520585495) | 35m01s total; 19 UI tests passed; native products were rebuilt on the independent Apple path | Correctness green; 120s and cross-workflow reuse FAIL |

The result is therefore a green correctness verdict only. The 120-second
objective remains FAIL, cross-job/native-product reuse remains unproven and is
recorded as FAIL for planning purposes, and a zero-work result for this
documentation/state-sidecar change was not observed. No performance gain is
claimed from this run.

The preceding measurements and first-attempt failures remain historical
evidence below; this addendum does not replace or recolor them.

---

## 1. 120s pipeline budget — FAIL, all classes

### Observed measurements (all wall-clock, trigger → terminal)

| Class | Observation | Run / job |
|---|---|---|
| Swift unit (green) | 25 min in-step (12:14→12:38), 13 min macOS queue before start | run 35597160258 |
| Swift unit (green) | 21 min 42 s job; 18 min 15 s unit checks | run 35598142741 (#1036) |
| Swift unit | 26+ min queued without starting (runner starvation) | run 35602945743 (#1041) |
| #1053 Swift unit (green) | 22 min 01 s; 78 Swift tests; repeated native compilation | [run 35651713880, job 106505596038](https://github.com/jackin-project/jackin/actions/runs/35651713880/job/106505596038) |
| #1053 post-merge Desktop (green) | 35 min 01 s; 19 UI tests; independent native rebuild | [run 35656263744, job 106520585495](https://github.com/jackin-project/jackin/actions/runs/35656263744/job/106520585495) |
| Full CI/Main | 18–32 min typical | session sample |
| Desktop merge | was ~35 min, S1 cut ~22 min via bootstrap scoping | — |
| Full matrix CI | 46 checks incl. macOS; queue + build dominate | runs 35598570563, 35611817500 |

No measured class is within the 120 s objective. The evidence above covers
Swift, full CI/Main, and Desktop. Release validation, scheduled Desktop,
cold-cache, and changed-dependency cohorts do not yet have comparable
representative measurements and remain unproven, not measured FAIL.

### Why it fails (ranked)

1. **Swift wall time (21–25 min).** The dominant native `Swift · Apple` job
   builds + tests its Swift surface serially. Full CI renders two Swift units;
   this evidence does not claim one job covers both. Proven genuine work, not
   mis-measurement.
2. **macOS queue starvation.** 13-min waits observed; jobs sit `queued` while
   Linux jobs drain. External capacity constraint (GitHub-hosted).
3. **Full-matrix fan-in.** The #1053 state-sidecar change selected all 40 units
   through the explicit fail-closed path. That selection is correct, but it
   proves neither a zero-work result for a genuine instruction-only change nor
   a reusable native product. D3's closed read contracts remain useful for
   declared closed units; they do not justify treating a changed global
   sidecar as no-work.
4. **Repeated setup.** S1 fixed the Desktop bootstrap (−22 min); per-unit
   duplication (tool install, cargo fetch, docker build) remains. S4's
   selection/read-contract capability landed in D3, while reuse and bootstrap
   slices remain unimplemented.

### What was tried

- S1 (Desktop bootstrap scoping): landed, −22 min. Only realized gain.
- Affected-selection narrowing: D3 landed generic closed read contracts. The
  #1053 live selection is an explicit 40/40 fail-closed result because the
  state sidecar changed; it is evidence of correct conservative behavior, not
  a no-work proof. Earlier 37/40 narrow-scope adjudications remain historical
  evidence and do not establish no-work for this candidate.
- S2, S3, and S5 (unit setup dedup, shared caches/cross-workflow reuse,
  cold-bootstrap slimming): scoped, not implemented.

### Recommendations (ordered, with dependencies)

1. **Split the Swift job** (biggest lever). Shard build vs test, and shard the
   test target set across parallel macOS jobs. Jobs do not share a filesystem;
   each must restore a validated DerivedData / SPM cache from the same narrow
   key. Measure the resulting queue and wall time; do not claim an
   estimate as a result. The two open schema-2 migrations (#1044 and #1052)
   overlap and cannot both land. Continue one converged carrier from current
   Velnor runtime products, retaining generic Apple discovery and removing the
   synthetic Swift escape hatch.
2. **Cache the Swift build inputs correctly.** Key SPM/DerivedData cache on
   Package.resolved + toolchain + target triple (narrow identity, not whole
   commit); verify hit rates from job logs before/after. Pairs with (1).
3. **boltffi-gen speedup.** FFI codegen sits on the critical path of Rust↔Swift
   units; profile it, cache its outputs keyed on generator input + pin.
4. **Cross-workflow dedup.** CI/PR, CI/Main, and Desktop rebuild overlapping
   units. Options in correctness order: (a) default-branch cache warming so
   main reuses PR-produced caches within GitHub's ref rules; (b) nextest
   archives with full provenance checks (producer repo/workflow/event/identity
   must match — b9 drill proved the rejection logic works, keep it).
5. **Establish Desktop candidate coverage now.** Main-only, supersedable
   Desktop runs are not a substitute for a pre-merge or merge-group verdict.
   Add a fork-safe candidate obligation first; optimize and shard it without
   dropping coverage.
6. **Re-measure per class after each slice** with the same run-link + queue +
   cache-byte discipline; keep the 120 s row FAIL until a full class sample
   passes.

No forecast is a completion claim. Keep each cohort FAIL until a representative
trigger-to-terminal measurement, including queue and cache export, passes.
Do not fake it with timeouts, coverage cuts, or warm-only reporting.

---

## 2. Six-nines reliability — NOT CLAIMED

First-attempt post-merge green is the objective; a single session (~10 mains)
cannot prove 99.9999%. Claiming it would be fabrication.

### What the sample actually shows

- First-attempt mains observed: mix of SUCCESS (65e9dfbd, 799f5774, 68edddab
  CI), FAILURE on external/flake causes (08713c9b openrouter flake, 87521a95
  cache-504/tirith/socket-flake, 52c5236c render flake), Renovate reds
  (EACCES, since fixed).
- Every failure preserved with attempt, none rerun-to-green, none recolored.
- Gate behavior correct in all cases: ci-required + Control/Required failed
  exactly when units failed, passed exactly when green.

### Recommendations

1. Keep the failure ledger + EXECUTION.md discipline: every main run gets a
   first-attempt verdict line; reruns are new rows, never edits.
2. Implement an automated, tested first-attempt collector and scheduled rollup:
   denominator = expected main commits and workflow obligations, not merely runs
   returned by one API page. Preserve reruns as appended attempts and classify
   missing jobs and cancellations explicitly. Under independent representative
   trials, zero failures need roughly 2.996 million successes for a one-sided
   95% upper bound of 10^-6; current evidence is far below that and correlated.
3. Drive the two flake sources below to zero first (they are the current
   reliability floor): cache-service 504 handling and the product timing tests.

---

## 3. Desktop verdicts for intermediate mains — STRUCTURALLY UNAVAILABLE

### Evidence

10 consecutive mains, every non-tip Desktop run `completed/cancelled`:

`65e9dfbd → 7eb2105d → 7e223f8c → c90bf147 → 08713c9b → 820ed5d6 → 799f5774 → 82ff0593 → 87521a95 → 68edddab`

Cause: `desktop-merge.yml` concurrency group cancels the prior main's run when
the next main pushes. Under rapid cadence (5+ mains/hour observed), an
intermediate Desktop may be cancelled before it finishes; it can finish when
cadence permits. Cancellations are by-design supersede, not failures — but
cancelled intermediate heads have zero Desktop evidence.

Combined with the b6b finding (desktop-merge has no `pull_request` trigger),
Desktop validation is main-only AND tip-only: the weakest coverage point in
the pipeline.

### Recommendations

1. **Short term (no behavior change):** record the gap explicitly per merge
   (done in EXECUTION.md) and treat tip-Desktop green as the Desktop signal.
   Do not backfill or rerun superseded Desktops.
2. **Medium term:** scope Desktop concurrency cancel to same-commit reruns
   only (cancel obsolete *attempts*, not obsolete *commits*), OR shard the
   Desktop suite so a run finishes faster than the merge cadence. Either is a
   Velnor generator change (desktop-merge.yml is generated) + Jackin regen.
3. **Long term (correct fix):** make Desktop fast enough (via §1 program) to
   run pre-merge, then the main-only gap closes entirely and supersede-cancel
   becomes harmless.

---

## 4. Active unresolved findings

### 4a. Velnor-self: phase composition + D19 self-hosting pin-lag

- Velnor's regeneration precondition clears phase identity, leaving its own
  `velnor-workflow` Rust unit unphased while ordinary Rust units are phased.
  This is a phase-composition defect, not an ownership exception.
- The D19 transition must keep producer source, published runtime, pin,
  generated tree, and ownership sidecar atomic. Main-event candidate polling
  cannot repair drift because no PR candidate exists for that event.
- Recommendation: make regeneration a composable typed precondition, preserve
  phase order and identity in both rendering paths, then promote a published
  runtime and regenerate consumers atomically. Non-PR acquisition must fail
  fast when no producer event can publish a candidate.

### 4b. Flaky product timing tests (product team)

| Test | Occurrences today | Signature |
|---|---|---|
| `host_daemon::…bounded_parented_rpc` (tests.rs:263) | 3 (b4-B job 106329218385 10.02 s; 87521a95 job 106382618559 10.06 s; +1 flagged) | OTLP export completion / ownership race after socket response, not socket RPC timeout |
| `parity_console_render_smoke_overview` (tests.rs:3039) | 1 (52c5236c job 106421702380) | "must contain expires in 30d" on byte-identical tree that was green at 68edddab |
| `openrouter_snapshot_…` (tests.rs:329) | 1 (08713c9b run 35605917622) | NeedsLogin vs Error |

- Recommendation: fix each structural cause. Pass a single reference instant
  through a console render and freeze boundary fixtures; preserve typed
  transport/HTTP/decode semantics for OpenRouter so only actual authentication
  responses yield `NeedsLogin`; and add isolated daemon lifecycle/export
  readiness and completion evidence with exact uniqueness assertions. Do not
  quarantine, blindly retry, relax counts, or inflate timeouts.

---

## Continuation checklist (for whoever picks this up)

- [ ] Converge #1044 and #1052 into one current-runtime schema-2 migration;
      preserve generic Apple discovery, Renovate behavior, and verified
      BoltFFI/XcodeGen producer-consumer materialization.
- [ ] Implement S2/S3/S5 reuse slices; run D3 selection adversarial/live probes;
      re-measure per class.
- [ ] Implement the tested first-attempt collector and scheduled rollup (#2.2).
- [ ] Establish Desktop candidate coverage and lossless per-main evidence.
- [ ] Repair Velnor D19 promotion and composable self-unit phases (#4a).
- [x] Land structural fixes and independent verification for all three product
      defects (#4b); verify their merged-main verdicts and retain first-attempt
      evidence.
