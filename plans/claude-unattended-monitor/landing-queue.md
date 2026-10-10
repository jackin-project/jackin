# Landing queue checkpoint

Recorded 2026-10-10. This is a durable checkpoint, not evidence that any queued
landing action has happened. Re-fetch branch and PR state before acting.

## Current refs and scope

- Main: `868ce535`.
- Usage candidate: `abd2260e`.
- Feature branch base: `ff9eb01f`.
- Existing prerequisite PR #1121 is unmerged: 203 commits / 4,439 files.
- The feature branch contains 25 commits / 219 files; the combined landing
  scope is 228 commits / 4,464 files.
- No usage PR exists yet. PR-body preparation is queued only; it has not
  created a PR. PR review, CI proof, any candidate-specific push, and merge
  remain pending. Earlier usage changes were pushed.

## Authorization and merge gates

- The user authorized landing the usage work on `main`.
- That authorization does not authorize prerequisite PR #1121. Clarification
  is pending on whether the user also authorizes landing that broad rollout.
  Do not land #1121 without explicit PR-specific authorization.
- Main requires DCO and the `Required` check. There is no merge queue. Use
  squash merge only; do not bypass requirements with admin privileges.
- PR #1121 currently has a `Plan` / `Required` failure because DCO action is
  required. That requirement has not been waived.
- Scoped CLI review found no confirmed defect; engine review found three
  required fixes: SGD45 authoritative readiness, late closed-period spend
  corrections, and provider invocation timing. Fixes and regressions are in
  progress; this head is not ready to land or evidence of safe live dispatch.

## Repository-specific PR instructions

- The canonical pull request template is
  `docs/PULL_REQUEST_TEMPLATE.md`, as directed by the root guide and `xtask`
  validation. Do not edit the generated `.github` template.
- Scope-auditor research says a clean monolithic port may be possible through
  manual integration. That integration is not complete; do not claim it has
  shipped.
- A usage-only port is now being implemented in isolated checkout
  `/private/tmp/jackin-usage-main-port`, branch
  `feat/claude-usage-monitor-main`, based on `868ce535`. It excludes the broad
  prerequisite rollout. Protocol, broker, auth/coordinator, consumers,
  telemetry, and documentation have bounded independent owners. Source fixes
  must finish before copying the affected monitor/coordinator modules.

## Claude monitor evidence and safety boundary

- All prior usage changes were pushed, and the installed binary was verified
  with a synthetic fixture. No real Claude account was verified.
- The latest Claude conversation audit found that a green doctor result with
  zero monitors does not mean tracking is active. An observer is needed to
  record evidence; it creates no spend goal or dispatch policy. Use only a
  genuine bound account or actual Claude session for observation.
- Treat a checkpoint request as a request to record state, not as blanket
  acknowledgement or additional authorization.
- Do not mutate Claude sessions, projects, settings, or authentication state.

## Next actions

1. Finish the engine and CLI scoped reviews and collect CI evidence for the
   exact candidate head.
2. Re-fetch main, candidate, PR #1121, checks, and review state. Resolve the
   unresolved authorization question before any action that would land #1121.
3. Prepare the usage PR using `docs/PULL_REQUEST_TEMPLATE.md`, then create it
   only after the candidate and its review/CI evidence are ready.
4. Land only the explicitly authorized scope after all required checks,
   DCO, approvals, and feedback are satisfied; use squash merge without
   bypasses.
