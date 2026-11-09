# A15 E2E final independent review

Date: 2026-11-10. Reviewer: independent final reviewer agent-58, not implementer agent-41.

Jackin commit: `b667957821df7008aeb6b6ed7be0f99902499e04`. Sentinel commit: `c3928c31497e5219a5e1457b43c7642fe940ef5f`.

Verdict: **CHANGES REQUIRED**.

## Blocking findings

### F1. Production performs setup effects before target validation

`e2e_command` awaits its boundary future before calling `run_e2e` at `crates/xtask/src/e2e.rs:158`. The production future builds jackin when `--jackin` is absent at `crates/xtask/src/e2e/production.rs:220`. It then resolves the gateway and connects to it at `crates/xtask/src/e2e/production.rs:250` and `crates/xtask/src/e2e/production.rs:266`. `E2eTarget::resolve` does not run until `setup::prepare` at `crates/xtask/src/e2e/setup.rs:44`.

An unavailable tuple or incomplete target can therefore cause a jackin build, gateway discovery, and a gateway connection before rejection. This violates the required fail-before-setup behavior. The test `selection_errors_precede_every_secret_gateway_scratch_and_image_effect` calls `run_e2e` with an already constructed fake `Boundaries`, so it cannot observe these earlier production effects.

Resolve the immutable target before calling `production`, pass that target into the run, and prove through `e2e_command` that an invalid target does not poll the boundary future. Keep the existing no-secret, no-scratch, and no-image assertions.

### F2. Invalid credential descriptors are echoed before redaction

`E2eCredential::parse` stores the rejected input in `TargetError::CredentialDescriptor` at `crates/xtask/src/e2e/target.rs:68`, `crates/xtask/src/e2e/target.rs:72`, and `crates/xtask/src/e2e/target.rs:86`. The error formats that string at `crates/xtask/src/e2e/target.rs:119`. Target resolution and its error output happen before `CredentialSession::open` creates the redactor at `crates/xtask/src/e2e/setup.rs:46`.

A raw token pasted into `--credential` is therefore returned and printed verbatim. The established CLI parser avoids this defect by returning a fixed expected-forms message at `crates/jackin/src/cli.rs:312`. Use a fixed message for the E2E parser and prove that rejected credential-bearing inputs do not appear in stderr.

## Independent red and green matrix

The exact approved 70-row sequence is in `.audit/a15-final-review-results.tsv`. Raw nextest output is in `.audit/a15-final-review-logs/`. The canonical validator is `.audit/a15-final-review-normalize.py`.

- 70 of 70 rows went red with exit 100 and exactly one failing test.
- 70 of 70 rows went green with exit 0 and exactly one passing test.
- No row is `NOT-RED`, a compile failure, an invalid red, or a green failure.
- Model, tuple, agent, provider, and adapter each failed the direct identity test with an equal-digest assertion under `A15_OMIT`. Each passed after the source was restored.
- The other 65 green runs used dormant mutation instrumentation. The final clean xtask suite passed after restoration.
- The first driver selection duplicated model and omitted `IT5.parsed_hosts`. I ran that row separately and required the normalized result to match all 70 approved identifiers in their original order. The corrected rerunnable driver is `.audit/a15-final-review-matrix.py`.

The direct failures name the omitted field and show identical left and right digests. Examples are `.audit/a15-final-review-logs/IT5.model.red.log` and `.audit/a15-final-review-logs/IT5.survivor.adapter.red.log`. Their corresponding green logs pass on restored source.

## Other verified behavior

The complete production and sentinel diffs were reviewed. `E2eTarget` exposes immutable references and no public mutation method. The canonical digest covers identity, complete behavior, and provider hosts while excluding credential material and runtime values. Schema-v3 parsing checks exact identity, digest, engine, mode, Docker, and step coverage. `ExitStatus::SkipOnly` exits 3, so SKIP cannot produce a successful run. `jackin-start` writes atomic version-2 `main` and `agent` records, and J04 requires a qualified direct child before its deadline.

Commands and observed results:

- `rtk mise exec -- mbx +1.99.1 nextest run -p jackin-xtask --no-fail-fast`: 245 passed, 0 skipped.
- `rtk mise exec -- mbx +1.99.1 nextest run -p jackin-image -E 'test(~jackin_start)' --no-fail-fast`: 19 passed.
- The two `jackin-session` build-selection tests passed.
- The two `jackin` CLI build-selection tests passed.
- `rtk mise exec -- mbx +1.99.1 xtask ci --only comments`: exit 0.
- `rtk /tmp/jackin-a15-final-review/target/debug/jackin role check .` in the sentinel worktree: exit 0.
- `rtk mise run test` in the sentinel worktree: first attempt killed by the 15-minute tool limit while running `probe/tests/j08.sh`; unrestricted rerun exited 0 in 970 seconds. Gitleaks, shellcheck, shfmt, hadolint, J08, setup-hook, and egress checks passed.

The final `jackin` binary was built from `b66795782` before the role check. Mutation worktrees are clean after restoration. No full G-FAST rerun was needed. No live gateway, sandbox, container, or real credential was used. No review documentation commit was created because the verdict is CHANGES REQUIRED.
