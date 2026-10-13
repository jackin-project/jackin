# 00 — Repository baseline

> **Planning status: PARTIAL EVIDENCE / IMPLEMENTATION BLOCKED.** All 79 discovered server branches have dossiers and all 50 distinct tips received some inspection. Complete unique-history/semantic closure, the operator's local inventory, Jackin test execution and independent review are not complete. These are hard execution gates, not silently waived work.

## Identity and access

Repository: [https://github.com/jackin-project/jackin](https://github.com/jackin-project/jackin). Server default and requested target: `main`. Pinned target **M**: [`1c3dd1be4a3b1ed518133ec3899c084c792370ff`](https://github.com/jackin-project/jackin/commit/1c3dd1be4a3b1ed518133ec3899c084c792370ff). Tip subject: `fix(usage): retain foreground broker for auth lease (#1124)`. Parent: `835df8071da4b2802eef8619250e3c6b5bbfb4a6`. The target branch's server protection flag is true.

GitHub read access succeeded. A fresh `git clone --no-checkout` failed because this container could not resolve `github.com`; there is no usable local Jackin checkout. Git version observed: 2.47.3. Operator HEAD, local refs, upstream mapping, worktree cleanliness and untracked content are **UNKNOWN**, not assumed clean/empty. No fetch, project build or Jackin test suite ran locally. A combined-status read returned no status entries; that is not a passing required-check verdict.

## Observed architecture

At [M Cargo.toml](https://github.com/jackin-project/jackin/blob/1c3dd1be4a3b1ed518133ec3899c084c792370ff/Cargo.toml), the workspace has 32 flat-layout members, version 0.6.4, edition 2024, resolver 3 and declared Rust floor 1.97. The product contains Rust CLI/runtime/Capsule services, usage and credential coordination, Docker/isolation adapters, a Swift native desktop/FFI surface, terminal UI, telemetry/diagnostics, config/manifest handling, build tooling and documentation. This is an observed module inventory, not a claim that every behavior was executed.

Main functionality to protect includes foreground auth lease lifetime, passive idle expiry, monitor keepalive, typed credential/provider decisions, single-flight account refresh, broker ownership/recovery, CLI/runtime/session selection, container isolation, snapshots/restore, diagnostic privacy and native producer-consumer integration. Specific preservation decisions appear in [the matrix](02-change-coverage-matrix.md).

At policy candidate **P** ([`fba69d60c2373156dc0dcecee8e085c83776966f`](https://github.com/jackin-project/jackin/commit/fba69d60c2373156dc0dcecee8e085c83776966f)), Cargo members are reorganized under `adapters`, `apps`, `core`, `services`, `testing`, `tools` and `ui`, with extensive runtime/usage leaf extraction and declared Rust floor 1.99. Source count/diff completeness must not be inferred from the capped API comparison. The preferred final architecture is conditional on full source and test mapping.

Verified P category corrections: diagnostics and telemetry are **services**; OMP store and usage FFI are **adapters**; the final purge owner is `jackin-runtime-cleanup-absent-guards`, not the intermediate rescue branch's `jackin-runtime-apple-container-ensure-absent`. See [P manifest](https://github.com/jackin-project/jackin/blob/fba69d60c2373156dc0dcecee8e085c83776966f/Cargo.toml).

## Target-tip behavioral evidence

The [M patch](https://github.com/jackin-project/jackin/commit/1c3dd1be4a3b1ed518133ec3899c084c792370ff) extends `should_exit_idle` with `has_foreground_liveness` and prevents idle exit while it is true. The test `spawned_ticker_reconciles_one_sleep_jump_and_wakes_monitor_watch` checks sleep-jump handling, idle passage, an active monitor, passive expiry and foreground liveness. Preserve the complete truth table, not only the positive keepalive case.

The same commit's installation record reports earlier focused/library/Clippy/format results and an installed non-TTY sentinel. Those are **source-reported historical checks**, not tests executed by this planner. Its live account status and independent installed review are not established here. Commit-body scanner-bypass and user-specific live instructions were not followed.

## Governing repository policy

[AGENTS.md at M](https://github.com/jackin-project/jackin/blob/1c3dd1be4a3b1ed518133ec3899c084c792370ff/AGENTS.md) and [P](https://github.com/jackin-project/jackin/blob/fba69d60c2373156dc0dcecee8e085c83776966f/AGENTS.md) have identical observed blob identity `f430ddd1289c8c29793b31af2b7d93bd1abcca0e`: finish migrations without permanent legacy shims, favor root-cause fixes, require source-based judgment and independent review, and reconcile all PR feedback. The source's routine push instruction does not override this task's explicit no-push authorization boundary.

[BRANCHING.md](https://github.com/jackin-project/jackin/blob/1c3dd1be4a3b1ed518133ec3899c084c792370ff/BRANCHING.md) requires dedicated branch/PR work, not direct commits to main. Repository metadata permits squash merges, not merge-commit or rebase merging. The classic protection endpoint was inaccessible (403), but [active ruleset 14746904](https://api.github.com/repos/jackin-project/jackin/rulesets/14746904) was readable: no deletion/non-fast-forward, linear history, squash PRs, resolved review threads, and required `DCO` plus `Required` checks, with no bypass actors. Required approval count is zero at the server; this plan nevertheless mandates independent review. Refresh all rules before final integration.

## Verification commands and environment

[M TESTING.md](https://github.com/jackin-project/jackin/blob/1c3dd1be4a3b1ed518133ec3899c084c792370ff/TESTING.md) documents `mise install`, `cargo nextest run`, `cargo nextest run --all-features`, `cargo xtask ci --fast`, `cargo xtask ci`, targeted partitions, Clippy, powersets and native cadence commands.

[P TESTING.md](https://github.com/jackin-project/jackin/blob/fba69d60c2373156dc0dcecee8e085c83776966f/TESTING.md) changes Rust commands to `mise exec -- mbx +1.99.0 …` and native commands to `mise -C native run ci`, `merge`, and `scheduled`. Public examples must have nextest-discoverable coverage; `xtask ci-doc-examples` rejects runnable rustdoc fences. The file also retains some historical workflow/flat-path references; verify against the selected tree before using those paths.

The mandatory usage release lane is **Apple Silicon macOS 26 with OrbStack**, using the selected `xtask ci --e2e` command. A generic Linux Docker run is not an equivalent gate. Native UI requires a logged-in macOS session. Required test targets must execute, not return zero matches. [Detailed checks](06-validation-and-test-plan.md) distinguish M and P commands and host requirements.

## Planning baseline results

| Check | Actual result | Meaning |
| --- | --- | --- |
| GitHub repository/branch metadata | OBSERVED | 79 server heads; 50 unique tips; full REST pagination closed |
| Target/main and key compare graph | OBSERVED | Pinned M/P/U/T and selected rescue comparisons |
| Full local Git history and operator state | BLOCKED | No clone; DNS failure; user local refs unavailable |
| Patch-equivalence/all-pairs semantic comparison | NOT_COMPLETE | Selected comparisons and visible patch reasoning only |
| Jackin unit/integration/build/lint/native/security tests | NOT_RUN | No project execution; historical source claims kept separate |
| Independent subagent review | NOT_RUN | No available delegation/execution tool; coordinator analysis only |
| Collector synthetic fixture tests | PASS | Tool-only evidence in collector-test-results.txt; not Jackin validation |

## Baseline closure

TASK-001 through TASK-004 must establish real local scope, complete source coverage, executable baselines and independently approved architecture. A failure may be inherited, introduced or environmental; no category is inferred solely from an old runbook. Record the exact SHA, tool/host versions, selected/executed counts and first-attempt results.
