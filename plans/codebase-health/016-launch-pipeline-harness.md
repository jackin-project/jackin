# Plan 016: Launch pipeline phase contracts + deterministic `run_launch_core` harness + pipeline benchmark

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-runtime/src/runtime/launch/`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P2
- **Effort**: L (multi-PR program; steps are sliceable)
- **Risk**: MED (the `jackin load` critical path)
- **Depends on**: none (coordinate with plan 015 in the same crate)
- **Category**: tech-debt (ownership/contracts)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Ownership item 1's acceptance is explicit: "Acceptance requires the real pipeline harness and benchmark, not helper-only substitutes." Today only 2 of the 10 named phases (grants validation, image classification) are typed by-value `#[must_use]` outputs; `run_launch_core` remains one monolithic async body carrying reasoned `too_many_lines` + `cognitive_complexity` allows whose own reason text says the extraction is pending; the deterministic harness at the `run_launch_core` boundary is explicitly "residual" (no `LaunchCore` builder — ~30 fields); and the Criterion benches cover isolated helpers, not the pipeline. The compiler therefore cannot catch a dropped or reordered phase, suite-A failure ordering is proven only end-to-end or helper-level, and pipeline-level performance is unmeasured. The [runtime-launch behavioral spec](../../docs/content/docs/reference/developer-reference/specs/runtime-launch.mdx) must stay synchronized.

## Current state

- Typed phases: `crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_phases.rs:43,63,101` — `GrantsValidated`, `ImagePhaseClassified` only.
- Monolith: `launch_pipeline/launch_core.rs:128-142` — `run_launch_core` with the two reasoned allows ("Until that slice lands, the inline shape preserves captured-locals across phases"); `LaunchCore` context struct at `:60-100` (~30 fields, no builder).
- Harness gap: `launch_pipeline/tests.rs:1-4` — "Full `run_launch_core` `LaunchCore` fixture is residual: constructing every field needs a dedicated builder…".
- Cleanup invariant already covered elsewhere: `LoadCleanup` arm/disarm + run-failure teardown tests (`launch/load_cleanup.rs:53-145`, `launch/tests.rs:5474`) — extend to the forced finalization/status/inspect error THROUGH the pipeline boundary (roadmap asks to "force a finalization/status/inspect error through that boundary and prove the armed `LoadCleanup` removes DinD/network resources before the error returns; audit every post-run/disarm teardown `?` path for the same invariant").
- Benches (helper-only): `crates/jackin-runtime/benches/launch_pipeline.rs:16-90`, `benches/launch_attach.rs:30-60`.
- Test doubles: canonical `FakeDockerClient`/`FakeRunner` in `crates/jackin-test-support/src/{docker.rs,runner.rs}`; real grant/profile/config fixtures used by `launch/tests.rs` (9214 lines).
- Phase list (roadmap): validation, materialization, trust, image resolution, environment resolution, run, wait, teardown, attach, cleanup.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Runtime tests | `cargo nextest run -p jackin-runtime` | pass |
| Launch module | `cargo nextest run -p jackin-runtime -E 'test(/launch/)'` | pass |
| Bench build | `cargo bench -p jackin-runtime --no-run` | builds |
| Spec gate | `cargo xtask docs specs` | exit 0 |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: `crates/jackin-runtime/src/runtime/launch/launch_pipeline/**` (phases, core, tests), a `LaunchCore` fixture builder (in `launch_pipeline/test_support.rs` or jackin-test-support if ≥2 consumers — follow the test_support rule in `crates/AGENTS.md`), `crates/jackin-runtime/benches/launch_pipeline.rs`, the runtime-launch spec page (citations for new tests), `crates/jackin-runtime/README.md`.

**Out of scope**: `image.rs` internals (plan 015); daemon/capsule (017); CLI handlers (022); changing observable launch behavior (the spec's INVs are the oracle — all preserved).

## Git workflow

Branch `refactor/launch-phase-contracts`; Conventional Commits; `git commit -s`; push per commit. This is a multi-slice program: builder+harness first (pure test win), then phase extractions one at a time, then bench. Each slice independently green.

## Steps

### Step 1: `LaunchCore` fixture builder + first real-boundary harness

Write a builder producing a fully-populated `LaunchCore` over `FakeDockerClient`/`FakeRunner` + real grant/profile/config fixtures (mine `launch/tests.rs` setup helpers for the fixture patterns — reuse, don't duplicate; if reuse requires moving helpers into a `test_support`, follow the fixture-registry rule). Add the first `run_launch_core`-boundary tests: happy path; suite-A failure ordering (the ordered validation failures the existing helper tests encode — replicate at the boundary); forced finalization/status/inspect error proving armed `LoadCleanup` removes DinD/network resources BEFORE the error returns.

**Verify**: `cargo nextest run -p jackin-runtime -E 'test(/launch_pipeline/)'` → new harness tests pass; the tests.rs "residual" comment deleted.

### Step 2: Teardown `?`-path audit

Enumerate every `?` between run start and cleanup disarm in `run_launch_core` (and helpers it calls); for each, either an existing test proves cleanup fires on that exit, or add one via the harness. Record the audit table in the PR description.

**Verify**: `cargo nextest run -p jackin-runtime` → pass; audit table complete.

### Step 3: Phase extraction (repeatable slice)

One phase at a time (order: trust → materialization → environment resolution → run → wait → teardown → attach → cleanup): extract a pure(ish) function returning a typed `#[must_use]` struct consumed by value by the next phase. Match the existing `GrantsValidated`/`ImagePhaseClassified` idiom in `launch_phases.rs`. After the last extraction, `run_launch_core` is a linear chain; delete its `too_many_lines`/`cognitive_complexity` allows.

**Verify per slice**: `cargo nextest run -p jackin-runtime` → pass; harness tests unchanged (boundary behavior stable). After final slice: `grep -n "too_many_lines\|cognitive_complexity" crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs` → no matches.

### Step 4: Pipeline Criterion scenario

Add a bench driving `run_launch_core` end-to-end over the builder (validation → materialization/image choice → run → attach/finalization → cleanup) with `FakeDockerClient` — wall-time of the orchestration logic itself. Keep existing helper benches.

**Verify**: `cargo bench -p jackin-runtime --no-run` → builds; a short `cargo bench -p jackin-runtime -- launch_pipeline --test` (Criterion test mode) runs.

### Step 5: Spec sync

Add/refresh citations in the runtime-launch spec for the new boundary tests (per the spec-gate citation format — see existing rows).

**Verify**: `cargo xtask docs specs` → exit 0; `cargo xtask ci --fast` → exit 0.

## Test plan

Steps 1–2 define the new suite; phase-extraction slices ride the existing 9k-line launch suite + harness as characterization. No assertion weakening anywhere.

## Done criteria

- [x] `LaunchCore` builder exists; boundary harness covers happy path, suite-A ordering, forced finalization/inspect error with cleanup-before-error proof
- [ ] All 10 phases typed `#[must_use]`, consumed by value; monolith allows removed
- [x] Teardown `?`-path audit complete with coverage
- [x] Pipeline-spanning Criterion bench exists and builds
- [ ] Spec citations updated; `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- A phase extraction changes observable ordering per the spec INVs (harness/spec tests redden) — revert the slice and report.
- The builder needs >a day of fixture archaeology (fields with no test precedent) — deliver the builder for the covered subset + a field inventory, and report.
- Captured-locals coupling between two phases resists by-value handoff without cloning large state — report the specific pair; a shared-context design call is the operator's.

## Maintenance notes

- New launch behavior must enter as a phase or a named step inside one — reviewers reject re-inlining.
- Plan 015 and this plan both shrink `jackin-runtime`'s largest files; the launch mega-test split (TEST-08) becomes natural follow-up after phases own their tests.

**Index deviation (audit 2026-07-15)**: demoted from DONE to IN PROGRESS — Done criteria not fully met; see implementer audit rollup.

## Teardown audit

| Armed region | Error routing | Coverage |
|---|---|---|
| Adoption through grants/materialization/environment | Each fallible branch runs grant-only or failed-setup cleanup before returning | `run_launch_core_suite_a_grant_failure_cleans_up_before_return`, `mid_pipeline_failed_setup_still_runs_cleanup` |
| Runtime launch | Failed status is best-effort, then cleanup runs before `launch_result?` | boundary happy/failure harness plus launch-suite run-failure tests |
| Attach/finalization | One `finalize_result` boundary catches every nested status/inspect/finalizer `?` and runs cleanup | `run_launch_core_finalize_error_runs_cleanup_before_return` |
| Final state classification/purge | One `teardown_result` boundary catches every nested `?` and runs cleanup; `InspectUnavailable` explicitly disarms first to preserve live resources | launch pipeline boundary suite and container-state matrix in the launch suite |

There are no fallible operations after the teardown boundary. `LoadCleanup::run` is idempotent, so an error after an arm that already cleaned is routed through it again without changing behavior.
