# CI/CD Reliability Execution Record

Goal: Jackin + upstream Velnor CI/CD — green pre-merge predicts green main, separate fmt/clippy/test steps, 120s pipelines, no repeated setup.

## Baselines (2026-09-21, parent-observed)

- Jackin root: `/Users/donbeave/Projects/tailrocks/jackin-project/jackin`, branch `main`, HEAD `fce94cea`, tree clean.
- Velnor root: `/Users/donbeave/Projects/tailrocks/velnor-project/velnor`, branch `codex/rolling-preview-legacy-migration-20260920` (NOT main), 1 commit ahead, untracked work present (`.firecrawl/`, `goal-finish-and-merge-ci-runtime-products.md`, `velnor-bastion-final-plan.md`, `__pycache__/`). DO NOT disturb; coordinate before checkout/branch moves.
- `gh` authenticated as `donbeave` (repo, workflow scopes).
- Mandatory jobs: run 35521080097/job 106105226160 (test failure `conformance_partial_success_is_not_retried`), run 35515575859/job 106090835001 (desktop-merge cancelled ~35m28s).

## Workstreams (workflow `cicd-discovery`, 6 agents + synthesis)

| Stream | Scope |
|---|---|
| failure-inventory | all retained runs/attempts, failure ledger |
| parity-gate | PR vs merge_group vs main vs desktop/release parity, final gate |
| hooks-hk-prek | hk vs prek decision + staged-snapshot hook design |
| velnor-arch | generator model, phase split sites, schema compat |
| perf-cache | critical path, cache audit, 120s budget per class |
| baseline-state | SHAs, pins, toolchains, regen procedure |

## Parent-verified facts (2026-09-21)

- Main HEAD `fce94cea` CI run 35521080097 FAILED; only failing job is 106105226160 `Rust · jackin-diagnostics`, step 29 `Run unit checks` (single opaque step — confirms combined fmt/clippy/test).
- Failure signature (log, /tmp/joblog.txt): `conformance_partial_success_is_not_retried` panicked at `crates/jackin-diagnostics/tests/wire_failure_support/mod.rs:51`, left 7 right 1. Fail-fast: 108/122 shown, rest cancelled.
- Prior main `a5e10227` run 35517379726 green → regression introduced by `fce94cea` (#1012 security fix). Prime parity case: why didn't PR validation catch it.
- Desktop merge cadence on `fce94cea` (35521079960) succeeded; cancelled 35m28s run was on `0163d1b7` (35515575859, event push).
- Open PRs: 1013 (active, in-progress run), 1002, 1004, 1005, 1007 (draft, `ci: pin the attested workflow runtime and regenerate` — directly relevant), 1009 (draft).
- Log fetch note: `gh api .../logs` needs `--allow-escape-sequences`.
- #1012 diff (a5e10227..fce94cea): 12 files, capsule/runtime/protocol + docs; NO diagnostics-crate changes → cross-crate behavioral regression. Failing assert is line 51 `testbed.logs().len()` (7 actual vs 1 expected) — extra exports.
- Generation input `.github-gen/velnor-workflow.toml`: schema 1, pin `4fa7a3a`; generated `project.toml` header says schema 2 (output version ≠ input schema). Rust unit commands render fmt → nextest → clippy (wrong order, one step).
- `ci-unit-rust.yml` (502 lines): single job `verify-github`; step `Run unit checks` shells `velnor-workflow run --config .github/ci/project.toml --scope … --unit …`. Phase split needs runtime CLI phase selection + generator multi-step emit.
- `desktop-merge.yml` (42 lines): concurrency `desktop-merge-<repo>-<ref>` cancel-in-progress → 14:43 push (a5e10227) cancelled 14:08 run (0163d1b7, job 106090835001). Same-ref serialization; perf/parity agents confirm + fix.
- Gating: `ci-required` (always(), needs all units) validates selected-must-pass / unselected-must-not-fail; no `merge_group` trigger anywhere. PR jobs gated on `plan.outputs.units` (change selection).
- Parity finding (PARENT-VERIFIED, diagnoser confirming mechanism): PR #1012 run 35519793543 RAN diagnostics, 122/122 PASS (job 106101859590, /tmp/prdiag.log). PR head tree == fce94cea tree; pr/full commands identical. NOT a selection gap: same test green on PR, red on main (105/122 position both). Leads: test nondeterminism (Testbed ports? global exporter? timing) or job env diff. Diagnoser redirected.
- #1012 metadata: head 0a053b15, base a5e10227 (= prior main), squash-merged fce94cea 15:55Z; PR run 15:30Z success.
- Diagnoser verdict (harvested): TRANSIENT FLAKE, #1012 exonerated (zero dep-closure overlap; vendor OTLP treats partial-success as Ok, no retry; unique 127.0.0.1:0 ports rule out cross-talk; 60ms test vs retry backoff incompatible). Local: 25/25 single, 5x122 suite, 60+24 instrumented — all green. Mechanism UNRESOLVED; next probe: instrument testbed LogsService::export with timestamp+payload-hash if recurs. Enabler: exact wire-count asserts over async batching pipeline with zero content inspection.
- Diagnostic rerun: run 35521080097 attempt 2 (failed-jobs-only) GREEN — transient confirmed; attempt-1 failure preserved in ledger.
- Slice `diag-harden`: `assert_wire_requests` helper (+73/-3, test-only) keeps exact counts, adds log-record content assert + rich mismatch dump (per-request spans/records/metrics, span/event/metric names, health snapshot). Parent-reviewed, fmt-clean, target test green with CI flags. Branch `cicd/integration`.
- Hooks ground truth: NO hooks exist (stock samples only, no core.hooksPath, no hk/prek config). prek 0.5.3 present as user-global mise shim; hk absent; neither pinned in repo.
- Perf ground truth: mise `[tools]` lists 15 `cargo:`-backed tools with `cargo.binstall=true` (binstall-first, source fallback) — source-fallback path is the desktop-merge compile-time suspect.
- Runtime model (Velnor main `runtime.rs`): `CiUnit.commands(lane, scope)` over 4 string arrays; `run --config/--scope/--unit`, NO phase selector. Phase split = typed phase model + runtime phase flag + generator multi-step emit (velnor-arch detail pending synthesis).
- Fragile pattern to remove: `prerequisite_commands` matches `command.contains(" clippy ")` + `replacen(" clippy ", " check ")` — exactly the substring detection the goal forbids. Non-full affected-scope units run only check prerequisites (no tests); #1012 diagnostics ran tests ⇒ was full unit.
- Velnor main RED: CI/main 35535449676 (59396040) + Preview failed; Runtime products green. Policy job 106143544517: committed tree differs from base-pin render (`project.toml`, `ci-main.yml`, `ci-pr.yml`, `preview.yml`, `release.yml`, generator-state — "rerun gen"), candidate-path fallback then 15-min timeout waiting for `velnor-workflow-candidate-8881e50b88c80c40-Linux-X64` (never published). Preview run 35535449623: `Resolve preview identity` failed. Runtime releases exist (latest `velnor-workflow-runtime-v1-92e3ae31fd276e74`); Jackin pin 4fa7a3a → af140ad4 exists. Upstream regen + candidate publication owned as follow-up (needs slot).

## Slices landed

- 2026-09-21: PR #1007 blocker fixed — `33b91891` fmt-only (`crates/jackin-xtask/src/desktop/tests.rs`), pushed to `codex/ci-performance-campaign`; xtask unit failed rustfmt --check → ci-required FAILURE. Enabling condition: no pre-commit fmt enforcement. Comment: PR #1007 comment 5752641444. VERIFIED: rerun 35537475111 xtask job success; zero failed jobs at 21:08Z check.
- 2026-09-21: Slice `diag-harden` → PR #1014 (branch `cicd/integration`, Jackin's integration branch): test-only wire-count actionability + EXECUTION.md. Attempt-2 green confirms transient; attempt-1 preserved.

## Decisions

(none yet — synthesis pending)

## Slices

(none yet)

## Evidence

- Discovery synthesis ref: pending.
