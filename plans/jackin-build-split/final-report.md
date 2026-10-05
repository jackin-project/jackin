# Initial Report

- Status: IN PROGRESS
- Date: 2026-10-05

This record captures the initial environment and static findings for Jackin Build Split. It does not report implementation completion.

The task branch `refactor/build-split` is based on `main` at `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. The original `main` worktree's unrelated `mise.lock` modification remains preserved.

No build, test, compilation, live account request, role launch, or Jackin split implementation acceptance review ran during this documentation checkpoint. No task PR was opened or merged.

## Current evidence state

- The Velnor workflow review is NOT APPROVED. It records missing coverage and accepts separate release and signing gates. See [reviews](reviews.md#ci-workflow-review).
- Architect PR [#479](https://github.com/jackin-project/jackin-the-architect/pull/479) passed exact-head static review. Role parsing, repository validation, and runtime remain NOT RUN. See [reviews](reviews.md#architect-integration-review).
- The account-consolidation migration has an exact-head Sol review and a P1 fixture finding. Required fixtures and MBX verification remain pending. Do not accept or merge it before they pass. See [reviews](reviews.md#account-consolidation-review).
- PR #1108 has an exact-head Sol review. It rejects obsolete generator and process changes, keeps only useful helpers as candidates, and requires scoped collector fixes. Existing check failures remain. See [branches](branches.md#independent-sol-review-disposition).
- Jackin build, test, and lint roots still call `cargo xtask`. The Rust-MBX and Mise-wrapper design remains pending. See [CI coverage](ci-coverage.md#task-invocation-gate).
- The account snapshot root-fix plan remains IN PROGRESS. Fixtures and tests are pending reviewed MBX. See [reviews](reviews.md#account-snapshot-follow-up).
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
