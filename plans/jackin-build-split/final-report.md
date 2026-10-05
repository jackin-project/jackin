# Initial Report

- Status: IN PROGRESS
- Date: 2026-10-05

This record captures the initial environment and static findings for Jackin Build Split. It does not report implementation completion.

The task branch `refactor/build-split` is based on `main` at `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. The original `main` worktree's unrelated `mise.lock` modification remains preserved.

The first main-source MBX attempt failed because the private chroot lacked `/etc/alternatives`. A second bounded proof succeeded at the same base SHA. Later reviewed runs collected three cold builds, one warm-cache build, and three no-op observations. The method review accepted only a narrow initial-main evidence gate: cold runs were contended, the warm series has one unexplained `aws-lc-sys` miss, and no-op runs wrote timing reports. No split comparison or post-integration baseline is accepted.

## Current evidence state

- The initial Velnor workflow review remains NOT APPROVED for unresolved Jackin coverage gaps. PR #55 merged; run `37261457091` passed all 20 jobs at the merge commit. A later current-main SHA has no verified CI run. See [CI coverage](ci-coverage.md#velnor-pr-55).
- Architect PR [#479](https://github.com/jackin-project/jackin-the-architect/pull/479) merged at `7b72b38fe1d66e35c0931899c53bf3719592bbcc`. The post-merge Sonar check failed, and the automated review summary completed after merge. The Jackin consumer source review passed, but Plan fails against pinned Velnor 0.1.0. Full role validation and role loading remain NOT RUN. See [reviews](reviews.md#merged-architect-and-consumer-status).
- The selected operation-ID migration now has source-review PASS at Jackin `17b2b1be6a58a0e34af6d8308df915d110f4a785`. The two immediate-predecessor input fixtures and an ignored rebake writer are committed. Generated goldens, writer execution, migration tests, and schema checks remain NOT RUN pending the exact MBX 1.22.0 sandbox review. See [migration fixture checkpoint](reviews.md#current-migration-source-and-fixture-checkpoint).
- PR #1108 has an exact-head Sol review. It rejects obsolete generator and process changes, keeps only useful helpers as candidates, and requires scoped collector fixes. Existing check failures remain. See [branches](branches.md#independent-sol-review-disposition).
- Jackin build, test, and lint roots still call `cargo xtask`. The Rust-MBX and Mise-wrapper design remains pending. See [CI coverage](ci-coverage.md#task-invocation-gate).
- The account snapshot root-fix plan remains IN PROGRESS. Fixtures and tests are pending reviewed MBX. See [reviews](reviews.md#account-snapshot-follow-up).
- Redaction source correction passed exact-head Sol review at `f52557d8ce10c2646d49f12f5de6ff7cbf2a1578`; Clippy and Cargo tests remain NOT RUN. See [reviews](reviews.md#nested-pem-and-current-correction).
- Velnor PR #55 merged at head `f17ebbc992da8549197f63d2aaaf1c317ed57426` via `3ec6f32b5bafa5fa34ce9aa22afd7cfe2e797132`; PR run `37260546503` and post-merge run `37261457091` succeeded. The latter had 20/20 jobs. The current later main SHA has no checked run. See [CI coverage](ci-coverage.md#velnor-pr-55).
- The initial-main MBX sequence has narrow method PASS for three cold builds, one warm-cache observation, and three validated no-op runs. Cold builds were contended; warm-cache had one unexplained `aws-lc-sys` miss; no-op runs wrote timing HTML and had unobserved scheduler waits. No split benefit, uncontended performance claim, or post-integration baseline is accepted. See [build results](build-results.md#narrow-initial-main-matrix-checkpoint).
- Crate names, consumers, dependency boundaries, and measured build targets remain IN PROGRESS. See [crate plan](crate-plan.md#usage-crate-naming-proposals).
- Codex route, restore-identity, and prompt source reviews passed. One bounded host-profile probe passed, but Cargo tests, Jackin discovery, workspace launch, and role execution remain NOT RUN. See [Debian results](debian-results.md#bounded-host-codex-route-probe) and [reviews](reviews.md#account-route-and-restore-source-reviews).
- Rule-bundle source `c72e25d384ce2d8a80cf584457ec4b28e619980b` was cherry-picked with attribution as `c8d20fb3a9660e1ed7819d53b3fbef410be43610`, changing three `jackin-agent-status` files. Focused tests remain NOT RUN pending controlled MBX scheduling. See [branch matrix](branches.md#account-and-capsule-consolidation-matrix).
- Consumer source review passed at Jackin `07f5ce7`; the sorted-task fix is at `41265a7`. CI still fails because pinned Velnor rejects the task configuration and generated CI lacks task jobs. No Jackin task PR is reported as merged.
- The current Jackin task-branch source head for this record is `17b2b1be6a58a0e34af6d8308df915d110f4a785`; tokenless BuildKit source passed review at `3b1a7789fe41e679eb9554e04862b6033cd82c94`, but Rust tests, Clippy, and image builds remain NOT RUN. Velnor PR #59's permission change merged by reported main `7ccc7617253343d6e59feb1727b068a0cc0e937e` with 19 successful checks and an exact permission-scope source PASS. Velnor PR #65 remains draft: its publisher source passed review at `ff41745394712604bddf94b6a4e9f1069100789e`, but run `37275543333` failed generated-workflow comparison, then Required failed and Rust jobs were skipped. See [CI coverage](ci-coverage.md).
- The owner reports that the bounded existing-profile Codex probe exited 0, with no MCP servers or tool item reported. Independent runtime evidence corroboration is pending. This does not prove Jackin discovery or Architect role execution. See [Debian results](debian-results.md#owner-reported-bounded-host-codex-route-probe).

## Open records

- [Checklist](checklist.md)
- [Branches](branches.md)
- [Crate plan](crate-plan.md)
- [Build results](build-results.md)
- [CI coverage](ci-coverage.md)
- [Debian results](debian-results.md)
- [Security and Codex review](reviews.md)

Update this report after the implementation and all required gates finish.
