# 06 — Validation and test plan

## Evidence status and acceptance rule

**Jackin tests, builds, security scans and platform checks were NOT_RUN during this planning session.** There is no executable Jackin checkout here. Historical results in source commits are not a current baseline. The supplied collector has separate [synthetic fixture results](evidence/collector-test-results.txt); those test this bundle's tooling only.

A check is `PASSED` only with an exact source/tree, host/tool versions, command, exit code, selected and executed test counts, and retained result. `FAILED` means an executed check did not meet its criteria. `BLOCKED` means a necessary environment, permission, source object or independent reviewer is unavailable. `NOT_RUN` means no attempt occurred. An observational read is `OBSERVED`, not a passing product test. Preserve the first attempt and distinguish a later retry/flaky pass; do not overwrite failed evidence with a green rerun.

Zero matching tests on a required surface is failure. Enumerate the selected cases before running filters: module extraction may have moved a named test into another package. A command documented in a source branch is not proof that it exists or succeeds in the integrated tree. TASK-003/004 must verify names against that revision's manifests, task definitions and `nextest list` before using them.

## Separate baseline contracts

Run in distinct worktrees at exact revisions. Do not install or execute code from archival WIP or probe branches before inspecting their build scripts and task definitions. Use clean disposable homes/state for tests; do not expose operator credentials to unreviewed code.

### Main M — 1c3dd1be4a3b1ed518133ec3899c084c792370ff

The [pinned main testing guide](https://github.com/jackin-project/jackin/blob/1c3dd1be4a3b1ed518133ec3899c084c792370ff/TESTING.md) supplies this baseline command family:

```sh
mise install
cargo metadata --no-deps --format-version 1
cargo nextest list --message-format json
cargo nextest run
cargo nextest run --all-features
cargo xtask ci --fast
cargo xtask ci
```

Execute one command at a time, retaining failure evidence; do not let a succeeding trailing command hide an earlier failure. Record the toolchain resolved by M's pins rather than applying P's compiler to M and calling that its original baseline. Main's foreground lease regression is named `spawned_ticker_reconciles_one_sleep_jump_and_wakes_monitor_watch`; first locate its current package/test binary, then require it to execute.

### Policy P — fba69d60c2373156dc0dcecee8e085c83776966f; selected post-migration contract

The [pinned policy testing guide](https://github.com/jackin-project/jackin/blob/fba69d60c2373156dc0dcecee8e085c83776966f/TESTING.md) specifies the following. This is not a claim these commands passed.

```sh
mise install
mise exec -- mbx +1.99.0 metadata --no-deps --format-version 1
mise exec -- mbx +1.99.0 nextest list --message-format json
mise exec -- mbx +1.99.0 xtask health --format json
mise exec -- mbx +1.99.0 nextest run
mise exec -- mbx +1.99.0 nextest run --all-features
mise exec -- mbx +1.99.0 xtask ci --fast
mise exec -- mbx +1.99.0 xtask ci
mise exec -- mbx +1.99.0 hack check -p jackin -p jackin-diagnostics -p jackin-capsule -p jackin-agent-status -p jackin-runtime --feature-powerset --all-targets --locked
```

Use the pinned tools installed by Mise; do not substitute ad hoc `cargo install` versions. `xtask health --format json` reports the verification map. Compare it to all actual workspace members and required binary/integration targets. Resolve U/T's own pinned testing instructions separately; their baselines must not be assumed equal to P's.

For targeted verification, use the exact package/command rows in [TASK-005 through TASK-015](05-execution-tasks.md). Examples supported by P's manifest are `jackin-usage-broker`, `jackin-usage-provider-claude`, `jackin-runtime-coordination`, `jackin-runtime-repo-cache`, `jackin-runtime-cleanup-absent-guards`, `jackin-diagnostics` and `jackin-omp-store`. Package-root membership does not establish where every old test moved.

## Required behavioral regression catalogue

The following rows are acceptance obligations. Where no corresponding test exists at the selected head, the task owner must add a nextest-discoverable regression and the independent reviewer must check that it fails without the intended behavior. Do not mark a row covered merely because a similarly named test exists.

| Owner | Required positive, negative and concurrency checks |
| --- | --- |
| TASK-007 | Foreground owner survives idle and sleep jumps; passive idle expiry remains; active monitor keeps alive; owner end releases correctly; wrong account/source/fingerprint or revoked identity cannot authorize; persisted uncertainty prevents stale credential reuse; unauthorized/unavailable paths make zero provider calls. |
| TASK-008 | Typed 401/403/429 are distinct from transport/decode/CLI text containing those numbers; auth decisions never parse a message as authority; current quota and zero-used windows retain evidence; unknown reset remains unknown; each window has its own provenance/freshness; no fabricated quota or reset. |
| TASK-009 | Supported predecessor state/wire fixtures migrate or fail explicitly; corruption/incompatibility does not silently reset data; observe-only has no runnable task or goal authorization; Desktop/Capsule adopt generations; timeout and rate-limit state stay shared without duplicate execution. |
| TASK-010 | Lock path/inode remains stable after release; reject unsafe path/owner/symlink cases; nested RepoLock ownership releases only at the last holder; independent paths do not contend accidentally; repository identity does not collapse distinct origins; purge refuses active or owned resources. |
| TASK-011 | Trim/reject empty model; validate reasoning effort; null/absent/default and existing dry-run keys remain intentional; actual launch and restore receive selections; backend choices and stale state cannot override the approved account or model silently. |
| TASK-012 | Each retry contains exactly the required log record; corrupt 0/1/2 distribution fails despite an equal total; secret canaries, PTY bytes, credentials and sensitive bodies do not leak to telemetry/errors; private OTLP testbed is the only test backend. |
| TASK-013 | OMP/WAL and config/recovery requirements from old records map to current behavior; interrupted writes and restore failure preserve last-good data; rejected conflicted snapshots contribute no merge markers; historical open items are verified or explicitly disposed, not assumed current defects. |
| TASK-014 | Staged-only dependency edits select their reverse dependents; HEAD↔index and index↔worktree union works; structural/unparseable inputs widen scope; comment-only changes stay safe; untracked/unstaged bytes are preserved by hook fixtures; moved tests/snapshots remain discoverable. |
| TASK-015 | CLI and desktop locate a packaged, matching sidecar; startup error context survives; fallback, if approved, has identical lease/CAS/capability constraints; native FFI is built before both consumers; UI harness counts are nonzero; no duplicate synthetic/native unit replaces lost coverage. |
| TASK-006 | Missing/cancelled/nonterminal/conflicting CI evidence cannot qualify green; producer/artifact/event/repository/workflow/head and ledger boundary are checked; multi-commit push differs from a Git-history denominator; exact cache keys invalidate changed semantic inputs; no unprovisioned or inherited compiler wrapper. |

The [33 seed decisions](02-change-coverage-matrix.md) add source-specific test rationale. The 79 OPEN remainder rows must generate further regression obligations where full history reveals additional meaningful changes. This catalogue is not claimed exhaustive until TASK-002 and TASK-018 close that work.

## Mandatory Apple Silicon macOS 26 + OrbStack gate

Usage discovery, coordinator/state, broker/relay transport, Capsule/Desktop refresh or backend launch assembly changes require the actual repository release-path lane:

```sh
mise exec -- mbx +1.99.0 xtask ci --e2e
mise exec -- mbx +1.99.0 nextest run -p jackin --features e2e --profile docker-e2e
```

Use M's corresponding Cargo command while measuring M. Outside the repository's PR-sync flow, inspect and run the selected `build-jackin-capsule -- --export` preparation documented in TESTING.md before Docker smoke tests. Do not claim E2E passed if the expected Capsule binary/image was never built or used.

`usage_broker_e2e` must execute under `docker-e2e`. Require JUnit evidence under `target/nextest/docker-e2e/` showing **2/20-client single-flight, owner-loss recovery, timeout ownership, Desktop/Capsule generation adoption, capability isolation, distinct-account concurrency, shared rate deadline/failure count, and unavailable-state zero-call assertions**. A Linux temp directory, generic Docker daemon, mock counter or process-only test does not satisfy this lane. Report macOS/OrbStack unavailable as BLOCKED. Keep durable results in the active approved PR/check or the local review packet, not committed raw logs/screenshots.

Native P commands:

```sh
mise -C native run ci
mise -C native run merge
mise -C native run scheduled
mise exec -- mbx +1.99.0 xtask desktop test
```

UI tests require a logged-in macOS session. Inspect native JUnit reports and expected harness counts; a silently skipped UI tier is not completion. Record actual producer/consumer/profile/deployment-floor/toolchain identity and verify packaging, release stamping and dSYM agreement where the selected tasks require them. Do not sign, notarize or publish artifacts without separate authorization.

## Policy, documentation, security and final-tree checks

Run the selected tree's full `xtask ci` and generated per-member acceptance contract, not only selected unit tests. Verify that build/type checks, strict Clippy, feature powersets, file/agent gates, snapshots, lock/dependency/advisory policy and relevant fuzz/benchmark compilation are actually selected. Execute dependency/security commands from the pinned contract; retain advisory database version and check time. No clean security result was obtained in planning.

```sh
mise exec -- mbx +1.99.0 xtask lint files
mise exec -- mbx +1.99.0 xtask lint agents
mise exec -- mbx +1.99.0 xtask roadmap audit
mise exec -- mbx +1.99.0 xtask docs repo-links
mise exec -- mbx +1.99.0 xtask research check
mise exec -- mbx +1.99.0 xtask ci-doc-examples --package jackin-xtask
git diff --check
python3 goal-references/tools/validate_bundle.py --root .
```

Run the example gate for every changed documented package, not just xtask. P requires examples mirrored into ordinary nextest tests instead of adding a second runnable rustdoc surface. Review each changed snapshot and reject pending snapshots. Inspect source files for unresolved conflict markers; sample/archive fixture matches require an explicit justified disposition. Regenerate CI **last**, from reviewed inputs using the selected published generator, then verify deterministic output and preserved handwritten files. Do not hand-edit ownership digests.

The policy branch's one-minute warm CI target is a **source-defined performance requirement**, not a measured achievement here. Measure each required job/pipeline, admission/step durations, cache state and forbidden dependency downloads/builds. Record cold misses honestly; do not inflate thresholds, delete slow obligations or convert missing work into success. A warm hit must match the semantic source/forward-dependency/toolchain/platform/feature contract, not just a similarly named artifact.

## Evidence required before completion

For each task record source IDs, destination commit, command/host, run identity, test binary and selected/executed counts, first attempt, retries, log/JUnit locations, independent reviewer and unresolved findings. Never include secrets or raw PTY capture in committed evidence. Re-run affected checks after every integrated wave and after final-head changes. Final audit compares all source behavior, tests, docs, configuration, assets, schemas and APIs to the result. Required checks attach to the exact final head; main's empty combined-status response or historical green commits are not approval.
