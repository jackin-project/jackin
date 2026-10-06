# Initial Report

- Status: IN PROGRESS
- Date: 2026-10-05

This record captures the initial environment and static findings for Jackin Build Split. It does not report implementation completion.

The task branch `refactor/build-split` is based on `main` at `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. The original `main` worktree's unrelated `mise.lock` modification remains preserved.

The first main-source MBX attempt failed because the private chroot lacked `/etc/alternatives`. A second bounded proof succeeded at the same base SHA. It is one build observation, not a comparable baseline or cache-reuse result. No tests, live account request, role launch, or Jackin split implementation acceptance review has completed. No task PR was opened or merged.

## Current evidence state

- The initial Velnor workflow review remains NOT APPROVED for unresolved Jackin coverage gaps. Velnor PR #55 later merged after its exact-head run succeeded; post-merge run `37261457091` is IN PROGRESS with 17 successes and one orchestrator check pending. See [CI coverage](ci-coverage.md#velnor-pr-55).
- Architect PR [#479](https://github.com/jackin-project/jackin-the-architect/pull/479) is open at marketplace fix `7db69b62f598a0971809ee4a006ad3f5477d0996`. Exact-head Sol review, consumer-contract update, full validation, and role loading remain pending. See [reviews](reviews.md#architect-integration-review).
- The account-consolidation migration has an exact-head Sol review and a P1 fixture finding. Required fixtures and MBX verification remain pending. Do not accept or merge it before they pass. See [reviews](reviews.md#account-consolidation-review).
- PR #1108 has an exact-head Sol review. It rejects obsolete generator and process changes, keeps only useful helpers as candidates, and requires scoped collector fixes. Existing check failures remain. See [branches](branches.md#independent-sol-review-disposition).
- Jackin build, test, and lint roots still call `cargo xtask`. The Rust-MBX and Mise-wrapper design remains pending. See [CI coverage](ci-coverage.md#task-invocation-gate).
- The account snapshot root-fix plan remains IN PROGRESS. Fixtures and tests are pending reviewed MBX. See [reviews](reviews.md#account-snapshot-follow-up).
- Redaction source correction passed exact-head Sol review at `f52557d8ce10c2646d49f12f5de6ff7cbf2a1578`; Clippy and Cargo tests remain NOT RUN. See [reviews](reviews.md#nested-pem-and-current-correction).
- Velnor PR #55 merged at head `f17ebbc992da8549197f63d2aaaf1c317ed57426` via `3ec6f32b5bafa5fa34ce9aa22afd7cfe2e797132`; run `37260546503` succeeded, including `Required`. Post-merge run `37261457091` has 17 successes; the orchestrator check remains in progress. See [CI coverage](ci-coverage.md#velnor-pr-55).
- One MBX source-build proof succeeded with 626 cache bypasses; cache causes remain under investigation. See [build results](build-results.md#reviewed-linker-v2-source-proof).
- Crate names, consumers, dependency boundaries, and measured build targets remain IN PROGRESS. See [crate plan](crate-plan.md#usage-crate-naming-proposals).
- Codex route, restore-identity, and prompt source reviews passed; their Cargo tests and runtime route remain NOT RUN. See [reviews](reviews.md#account-route-and-restore-source-reviews).
- Rule-bundle source `c72e25d384ce2d8a80cf584457ec4b28e619980b` was cherry-picked with attribution as `c8d20fb3a9660e1ed7819d53b3fbef410be43610`, changing three `jackin-agent-status` files. Focused tests remain NOT RUN pending controlled MBX scheduling. See [branch matrix](branches.md#account-and-capsule-consolidation-matrix).
- The task branch contains documentation progress commits. It has no task PR and no merge.

## Open records

- [Checklist](checklist.md)
- [Branches](branches.md)
- [Crate plan](crate-plan.md)
- [Build results](build-results.md)
- [CI coverage](ci-coverage.md)
- [Debian results](debian-results.md)
- [Security and Codex review](reviews.md)

Update this report after the implementation and all required gates finish.
