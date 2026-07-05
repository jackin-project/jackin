# Plan 008: Add the testability seam so the full status tick runs in CI on any host

> **Executor instructions**: This is the verification prerequisite for plans 003 and 004. Run every
> verification command. Update the README row when done.
>
> **Drift check**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-capsule/src/session.rs crates/jackin-capsule/src/agent_status/process.rs crates/jackin-capsule/src/agent_status/arbitrate.rs`

## Status

- **Implementation status**: DONE in PR 714 (`ProcessSampler` lives in `jackin-agent-status`, capsule exposes
  `advance_status_with_process_sampler`, and host-portable tests drive the collect→arbitrate→publish path)
- **Priority**: P1 (unblocks safe changes to 003/004)
- **Effort**: M
- **Risk**: LOW (test/abstraction only)
- **Depends on**: none (blocks 003, 004)
- **Category**: tests
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

`advance_status` — the function that assembles every evidence source into an `EvidenceSnapshot`, arbitrates,
applies the watchdog, debounces, and publishes — has **zero** test coverage. Every *component* is unit-tested
with injected inputs, so CI stays green, but the *assembly* (the exact wiring that plans 003/004 fix) is never
executed by any test. Worse, the `/proc` physics sampler and the authority-wins arbitration branch are gated
behind `foreground_is_agent`, which is only ever `true` from the Linux `/proc` sampler — so on the dev platform
(macOS) the highest-value branch is **never exercised locally**, and the tests compensate by hard-coding
`foreground_is_agent: true`. Result: wiring breaks in identification, cadence, and evidence collection ship
green. Root cause: the integration boundary has no injection seam, so it is untestable and therefore untested.
This plan adds the seam first, so 003/004 can be changed with a test that proves the fix.

## Current state

- `advance_status` (`crates/jackin-capsule/src/session.rs:969-1042`) assembles `sample_process_evidence` +
  screen `evaluate_with_virtuals` + `osc_evidence` → `EvidenceSnapshot` → `arbitrate` → `apply_watchdog` →
  `debounce` → `publish_raw`. Grep for `advance_status` across all `tests.rs` → **no references**.
- `crates/jackin-capsule/src/agent_status/arbitrate/tests.rs:8-22` — `base_snapshot()` hand-builds an
  `EvidenceSnapshot` with `foreground_is_agent: true` hardcoded and `authority: None`, then calls the pure
  `arbitrate()`. The collect→publish plumbing is never run.
- `crates/jackin-capsule/src/agent_status/process.rs:88-90` — `physics_available()` = `cfg!(target_os="linux")`;
  off-Linux the `procfs` shim returns empty (`process.rs:16-58`), so `session.rs:919` returns
  `ProcessEvidence::default()` (`foreground_is_agent=false`).
- `crates/jackin-capsule/src/agent_status/arbitrate.rs:73-76` — `fresh_authority` requires
  `snapshot.process.foreground_is_agent`, so authority can never win off-Linux.

## Scope

**In scope:** a trait/seam for process evidence + a way to drive a full `advance_status` tick with injected
inputs, plus tests. **Out of scope:** changing arbitration *behavior* (plans 003/004); the render layer.

## Steps

### Step 1: Abstract the `/proc` sampler behind a trait with an in-memory double

Introduce a `ProcessSampler` trait (methods the real `/proc` code implements: foreground-agent detection,
descendant count, CPU deltas) with the real Linux impl and a test/in-memory impl fed synthetic `ProcessInfo`
/ foreground data. Route `sample_process_evidence` through it. This lets the physics + authority-wins path run
in CI on **any** host (including macOS dev machines), not just in-container Linux.

### Step 2: Give `advance_status` an injectable evidence path

Add a seam so a test can drive one full tick with (a) synthetic screen text, (b) an injected
`ProcessEvidence` (via Step 1's double), and (c) optional OSC/authority inputs, then assert the **published**
`session.state` and `session.status.report()`. Keep the production path unchanged (the seam is an injection
point, not a behavior change). Options: parameterize `advance_status` over the `ProcessSampler`, or expose a
`advance_status_with(snapshot_provider)` that the production `advance_status` calls with the real provider.

**Verify**: `cargo check -p jackin-capsule --all-targets` → exit 0.

### Step 3: Tests that exercise the assembly (not the mocks)

Add integration tests through `advance_status`:
- Screen text that matches a blocked rule + `foreground_is_agent=true` → published state Blocked.
- Screen text matching idle + quiet physics → Idle → (unseen) Done.
- A fresh authority (opencode-style) with `foreground_is_agent=true` → authority wins (this exercises the
  branch that never ran on macOS).
- Unknown when no evidence — assert it does **not** silently become something else.

**Verify**: `cargo nextest run -p jackin-capsule -E 'test(/advance_status|status_tick|process_sampler/)'` →
pass, and these tests run/pass on the dev host (macOS) — not `cfg(linux)`-gated.

## Done criteria

- [x] A `ProcessSampler` trait with a real Linux impl + an in-memory test double
- [x] `advance_status` can be driven end-to-end in a test with injected evidence; production path unchanged
- [x] Tests exercise the collect→arbitrate→publish assembly, including the authority-wins branch, on any host
- [x] `cargo nextest run -p jackin-capsule` green (new tests included); clippy clean
- [x] `plans/agent-status/README.md` row updated

## STOP conditions

- Threading the sampler seam requires touching many call sites in a way that risks behavior change — keep the
  production wiring byte-identical (the seam is additive); if you can't, report the surface before proceeding.
- The authority-wins test reveals authority *already* can't win for a reason beyond the `foreground_is_agent`
  gate — that's a finding for plan 004; record it.

## Maintenance notes

- This is the safety net for the whole subsystem: after it, a regression in identification/cadence/collection
  fails a test instead of shipping green. A reviewer should require new evidence-path code to add an
  `advance_status`-level test, not just a component unit test.
- Do 008 before 003 and 004 — those change the assembly this plan makes testable.
