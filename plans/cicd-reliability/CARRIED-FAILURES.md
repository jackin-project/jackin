# Carried failures — details and recommendations

Jackin CI/CD goal, session 2026-09-21. Everything below is FAIL / not-claimed / owned-out
at goal completion. Evidence record:
[EXECUTION.md](/Users/donbeave/Projects/tailrocks/jackin-project/jackin/plans/cicd-reliability/EXECUTION.md).

---

## 1. 120s pipeline budget — FAIL, all classes

### Observed measurements (all wall-clock, trigger → terminal)

| Class | Observation | Run / job |
|---|---|---|
| Swift unit (green) | 25 min in-step (12:14→12:38), 13 min macOS queue before start | run 35597160258 |
| Swift unit (green) | ~55 min in-step under contention | run 35598142741 (#1036) |
| Swift unit | 26+ min queued without starting (runner starvation) | run 35602945743 (#1041) |
| Full CI/Main | 18–32 min typical | session sample |
| Desktop merge | was ~35 min, S1 cut ~22 min via bootstrap scoping | — |
| Full matrix CI | 46 checks incl. macOS; queue + build dominate | runs 35598570563, 35611817500 |

No measured class is within 2 orders of magnitude of 120 s. Nothing was exempted
to get here: desktop, main, release-validate, cold-cache, and changed-dependency
runs were all measured FAIL.

### Why it fails (ranked)

1. **Swift wall time (15–57 min).** Single `Swift · Apple` job builds + tests the
   whole Swift surface serially. Proven genuine work, not mis-measurement.
2. **macOS queue starvation.** 13-min waits observed; jobs sit `queued` while
   Linux jobs drain. External capacity constraint (GitHub-hosted).
3. **Full-matrix fan-in.** `plans/**` and other unwatched paths force FULL scope
   (28–46 units); b8 probe live-proved a base-lag confound triggering full scope.
4. **Repeated setup.** S1 fixed the Desktop bootstrap (−22 min); per-unit
   duplication (tool install, cargo fetch, docker build) remains — see §10
   program below, S2–S5 unimplemented.

### What was tried

- S1 (Desktop bootstrap scoping): landed, −22 min. Only realized gain.
- Affected-selection narrowing: proven genuine (37/40 narrow-scope runs
  adjudicated real, not drift) but wall-ineffective — Swift still dominates.
- S2–S5 (unit setup dedup, shared caches, cross-workflow reuse, cold-bootstrap
  slimming): scoped, not implemented.

### Recommendations (ordered, with dependencies)

1. **Split the Swift job** (biggest lever). Shard build vs test, and shard the
   test target set across parallel macOS jobs sharing one warmed DerivedData /
   SPM cache. Expected: 25 m → ~8–12 m wall. Depends on: #1044 landing first
   (external PR migrating Apple CI to the Velnor generic recipe — same files;
   building on the pre-#1044 shape wastes the work).
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
5. **Gate desktop-merge scope.** The 35-min suite cannot move pre-merge until
   (1)–(4) land; until then keep it main-only (current state) and accept the
   b6b-documented parity gap explicitly.
6. **Re-measure per class after each slice** with the same run-link + queue +
   cache-byte discipline; keep the 120 s row FAIL until a full class sample
   passes.

Realistic outlook: (1)+(2) get CI/PR to ~10 min, not 120 s. Sub-120 s needs
either macOS capacity that doesn't exist on GitHub-hosted, or a fundamentally
narrower per-PR contract (fewer units, not faster units) — a product decision,
not an engineering tweak. Do not fake it with timeouts, coverage cuts, or
warm-only reporting.

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
2. Add a lightweight weekly rollup (script over `gh run list`): denominator =
   main-push CI/Main first attempts; count success/fail/cancel-by-cause.
   Six-nines needs ~1M samples — the mechanism matters, not the current number.
3. Drive the two flake sources below to zero first (they are the current
   reliability floor): cache-service 504 handling and the product timing tests.

---

## 3. Desktop verdicts for intermediate mains — STRUCTURALLY UNAVAILABLE

### Evidence

10 consecutive mains, every non-tip Desktop run `completed/cancelled`:

`65e9dfbd → 7eb2105d → 7e223f8c → c90bf147 → 08713c9b → 820ed5d6 → 799f5774 → 82ff0593 → 87521a95 → 68edddab`

Cause: `desktop-merge.yml` concurrency group cancels the prior main's run when
the next main pushes. Under rapid cadence (5+ mains/hour observed), only the
tip's Desktop can ever finish. Cancellations are by-design supersede, not
failures — but intermediate heads have zero Desktop evidence.

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

## 4. Owned-out items (not this goal's to fix)

### 4a. Velnor-self: unphased Rust unit + D19 self-hosting pin-lag

- Velnor's own `velnor-workflow` Rust unit still runs fmt+clippy+test in one
  step (Jackin is phased; Velnor-self is not). GAP recorded, owner = Velnor
  workflow owner (touches their release process).
- Velnor main CI red since #985: Policy fails on 6-file generated drift
  because the self-pin (1e454958) lags renderer changes — proven pre-existing
  on f406baff (run 35600683746, job 106335785491) vs parent 447c0f83
  (run 35600604077, job 106335526425), byte-identical cause. Does not block
  runtime publication or consumer adoption.
- Recommendation: Velnor owner runs the pin-advance chore (same class as
  fff18da8→#999) and phases the self unit. Jackin-side needs nothing.

### 4b. Flaky product timing tests (product team)

| Test | Occurrences today | Signature |
|---|---|---|
| `host_daemon::…bounded_parented_rpc` (tests.rs:263) | 3 (b4-B job 106329218385 10.02 s; 87521a95 job 106382618559 10.06 s; +1 flagged) | socket RPC timeout ~10 s |
| `parity_console_render_smoke_overview` (tests.rs:3039) | 1 (52c5236c job 106421702380) | "must contain expires in 30d" on byte-identical tree that was green at 68edddab |
| `openrouter_snapshot_…` (tests.rs:329) | 1 (08713c9b run 35605917622) | NeedsLogin vs Error |

- Recommendation: product/usage-lane owners quarantine-then-fix (these fail
  green code at random; each occurrence reds a main). CI-side must NOT add
  blind retries or inflate timeouts — that hides the signal. Suggested fixes:
  deterministic fixtures for render tests, bounded-retry assertions or
  longer-but-asserted windows for socket tests, hermetic openrouter stubs.

---

## Continuation checklist (for whoever picks this up)

- [ ] Land/wait for external #1044 (Apple CI → generic recipe), then start
      Swift split (#1.1) + cache (#1.2).
- [ ] Implement S2–S5 reuse slices; re-measure per class.
- [ ] Set up the weekly first-attempt rollup (#2.2).
- [ ] Decide Desktop concurrency scope (#3.2) with the Velnor owner.
- [ ] Nudge Velnor owner on D19 pin-advance + self-unit phasing (#4a).
- [ ] Nudge product team on the three flaky tests (#4b).
