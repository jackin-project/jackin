# Initial Report

- Status: IN PROGRESS
- Date: 2026-10-05

This record captures the initial environment and static findings for Jackin Build Split. It does not report implementation completion.

The task branch `refactor/build-split` is based on `main` at `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. The original `main` worktree's unrelated `mise.lock` modification remains preserved.

The first main-source MBX attempt failed because the private chroot lacked `/etc/alternatives`. A second bounded proof succeeded at the same base SHA. It is one build observation, not a comparable baseline or cache-reuse result. Later launcher v3 security failed, v4 was not reviewed, and v5 failed its compiler-version guard before compilation. No comparable measurement matrix ran.

## Current evidence state

- The initial Velnor workflow review remains NOT APPROVED for unresolved Jackin coverage gaps. PR #55 merged; run `37261457091` passed all 20 jobs at the merge commit. A later current-main SHA has no verified CI run. See [CI coverage](ci-coverage.md#velnor-pr-55).
- Architect PR [#479](https://github.com/jackin-project/jackin-the-architect/pull/479) merged at `7b72b38fe1d66e35c0931899c53bf3719592bbcc`. The post-merge Sonar check failed, and the automated review summary completed after merge. The Jackin consumer source review passed, but Plan fails against pinned Velnor 0.1.0. Full role validation and role loading remain NOT RUN. See [reviews](reviews.md#merged-architect-and-consumer-status).
- The account-consolidation migration has an exact-head Sol review and a P1 fixture finding. Required fixtures and MBX verification remain pending. Do not accept or merge it before they pass. See [reviews](reviews.md#account-consolidation-review).
- PR #1108 has an exact-head Sol review. It rejects obsolete generator and process changes, keeps only useful helpers as candidates, and requires scoped collector fixes. Existing check failures remain. See [branches](branches.md#independent-sol-review-disposition).
- Jackin build, test, and lint roots still call `cargo xtask`. The Rust-MBX and Mise-wrapper design remains pending. See [CI coverage](ci-coverage.md#task-invocation-gate).
- The account snapshot root-fix plan remains IN PROGRESS. Fixtures and tests are pending reviewed MBX. See [reviews](reviews.md#account-snapshot-follow-up).
- Redaction source correction passed exact-head Sol review at `f52557d8ce10c2646d49f12f5de6ff7cbf2a1578`; Clippy and Cargo tests remain NOT RUN. See [reviews](reviews.md#nested-pem-and-current-correction).
- Velnor PR #55 merged at head `f17ebbc992da8549197f63d2aaaf1c317ed57426` via `3ec6f32b5bafa5fa34ce9aa22afd7cfe2e797132`; PR run `37260546503` and post-merge run `37261457091` succeeded. The latter had 20/20 jobs. The current later main SHA has no checked run. See [CI coverage](ci-coverage.md#velnor-pr-55).
- One MBX source-build proof succeeded with 626 cache bypasses; cache causes remain under investigation. Measurement launcher v5 passed exact-artifact review but failed its compiler-version guard before any compile. No performance matrix ran. See [build results](build-results.md#reviewed-linker-v2-source-proof).
- Crate names, consumers, dependency boundaries, and measured build targets remain IN PROGRESS. See [crate plan](crate-plan.md#usage-crate-naming-proposals).
- Codex route, restore-identity, and prompt source reviews passed; their Cargo tests and runtime route remain NOT RUN. See [reviews](reviews.md#account-route-and-restore-source-reviews).
- Rule-bundle source `c72e25d384ce2d8a80cf584457ec4b28e619980b` was cherry-picked with attribution as `c8d20fb3a9660e1ed7819d53b3fbef410be43610`, changing three `jackin-agent-status` files. Focused tests remain NOT RUN pending controlled MBX scheduling. See [branch matrix](branches.md#account-and-capsule-consolidation-matrix).
- Consumer source review passed at Jackin `07f5ce7`; the sorted-task fix is at `41265a7`. CI still fails because pinned Velnor rejects the task configuration and generated CI lacks task jobs. No Jackin task PR is reported as merged.

## Open records

- [Checklist](checklist.md)
- [Branches](branches.md)
- [Crate plan](crate-plan.md)
- [Build results](build-results.md)
- [CI coverage](ci-coverage.md)
- [Debian results](debian-results.md)
- [Security and Codex review](reviews.md)

Update this report after the implementation and all required gates finish.
