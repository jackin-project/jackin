# Plan 046: Scheduled chaos variant for the Docker-backed E2E — seeded faults, survival invariants

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat fabe88406..HEAD -- crates/jackin/tests .github/workflows/hygiene.yml`
> Plans 022/035 append other jobs to hygiene.yml — expected drift; append
> beside them. Changes to `crates/jackin/tests/dind_e2e*`: compare excerpts,
> STOP on mismatch.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (E2E flakiness risk — mitigated: scheduled-only lane, seeded determinism, never a PR gate)
- **Depends on**: none (the named blockers in the old index row — clock seam 024, daemon ports — were analyzed and do not apply: chaos kills real containers with a logged seed and injects externally)
- **Category**: tests
- **Planned at**: commit `fabe88406`, 2026-07-09

## Why this matters

Roadmap Phase 3 (Deterministic simulation item 4): "Extend the existing Docker-backed E2E with a scheduled chaos variant that kills containers, drops the control socket, and sends SIGKILL to the capsule at randomized-but-seeded points, asserting the invariants that must survive: no orphaned containers, no stale state dirs, correct cleanup classification, reattach either works or fails clean. Log the seed so any failure replays." The E2E substrate exists and is mature; no chaos variant exists. These are exactly the teardown/cleanup classes that reach operators as orphaned `jk-*` containers and stale state dirs — the highest-value invariants a scheduled lane can hold.

## Current state

All facts verified at `fabe88406`.

- E2E substrate: `crates/jackin/tests/dind_e2e.rs` + helper modules `crates/jackin/tests/dind_e2e/{common.rs, util.rs, pty_runner.rs, transcript.rs, fixtures.rs, diagnostics.rs}` — Docker-in-Docker harness behind `--features e2e`, run via `cargo nextest run -p jackin --features e2e --profile docker-e2e` (CONTRIBUTING.md documents the lane; `cargo xtask ci --e2e` wraps it: checks Docker, builds/exports the local capsule, runs the profile). Read `dind_e2e.rs` and `common.rs` fully before writing any code — the container naming, label conventions, state-dir layout, and teardown helpers there are the vocabulary this plan reuses.
- Scheduled lane to extend: `.github/workflows/hygiene.yml` — `scheduled-hygiene` job (:49-92) shows the repo's job shape: pinned `actions/checkout`, rustup cache, `jdx/mise-action` with `install_args`, cargo-registry cache composite, `Swatinem/rust-cache`, then run steps. Workflow-authoring rules in `.github/AGENTS.md` bind this plan: all tools via mise, `${{ github.token }}` for same-repo reads, `${{ secrets.GH_READONLY_TOKEN }}` only for mise-action, job-level env only.
- Cleanup-classification surface: launch teardown classifies outcomes (plan 008 targets one finalization gap; plan 033 characterizes teardown ordering in-process). This plan asserts the *end state* from outside: after a chaos run, `docker ps -a` filtered by the jackin label must be empty (or only deliberate survivors), and the state directory tree must contain no orphaned per-container dirs.
- Determinism: no `rand` dependency is guaranteed in the test tree — do NOT add one. A 15-line xorshift64 over a `u64` seed suffices; seed from `JACKIN_CHAOS_SEED` env when set, else derive from a fixed default per scenario. Print the seed FIRST in every test (`eprintln!`-equivalent through the harness's logging — tests may use stdout freely under nextest; the workspace print lints carve out tests via clippy.toml `allow-print-in-tests`).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Compile the e2e tree | `cargo check -p jackin --features e2e --all-targets` | exit 0 |
| Run chaos locally (Docker running) | `cargo nextest run -p jackin --features e2e --profile docker-e2e -E 'test(chaos)'` | all pass, seed printed |
| Full existing e2e (regression) | `cargo xtask ci --e2e` | green |
| Workflow lint | `gh workflow view Hygiene` (after push) | job listed |

## Scope

**In scope**:
- `crates/jackin/tests/dind_e2e/chaos.rs` (new helper module: fault schedule, xorshift, invariant asserts) — declared from `dind_e2e.rs` beside the existing helper mods
- Chaos test fns in `crates/jackin/tests/dind_e2e.rs` (or the file layout the existing suite uses for test fns — match it)
- `.github/workflows/hygiene.yml` — one new `dind-chaos` job (append; do not restructure)
- `TESTING.md` — chaos lane row (how to run, how to replay a seed)

**Out of scope** (do NOT touch):
- Production teardown/cleanup code — if an invariant fails, that is a bug report (file it in the PR body + defect ledger when 017 lands), NOT something this plan fixes.
- turmoil/proptest-state-machine simulation (stays sequenced behind the daemon port decomposition).
- PR-time CI — the lane is scheduled + workflow_dispatch only.
- The existing e2e tests and fixtures.

## Git workflow

- Branch off `main`: `test/dind-chaos-lane`.
- Conventional Commits (`test(e2e): …`, `ci: …`), `-s`, push per commit. PR to `main`; do not merge. The lane job is push-main/schedule-gated → smoke-test via `gh workflow run Hygiene --ref <branch>` before calling it done (`.github/AGENTS.md` rule).

## Steps

### Step 1: Chaos helper module

Create `dind_e2e/chaos.rs`: `struct ChaosRng(u64)` (xorshift64, `next_range(n)`), `fn seed() -> u64` (env `JACKIN_CHAOS_SEED` else default constant), `enum Fault { KillContainer, SigkillCapsule, DropControlSocket }`, `fn schedule(rng, faults, window)` picking fault+delay points. Invariant helpers, built on `common.rs`/`util.rs` vocabulary after reading them: `assert_no_orphaned_containers()` (docker ps -a by the harness's label/name prefix), `assert_no_stale_state_dirs(state_root)` (no per-container dirs for containers that no longer exist), `assert_cleanup_classified(diagnostics)` (the run diagnostics/transcript records a cleanup outcome — reuse `diagnostics.rs` parsing).

**Verify**: `cargo check -p jackin --features e2e --all-targets` → exit 0.

### Step 2: Three chaos scenarios

Each: start a launch through the existing harness (`pty_runner.rs` drives the PTY), print the seed, apply one scheduled fault, then assert ALL invariants plus the scenario-specific one:

1. `chaos_kill_container_mid_session` — `docker kill <container>` at a seeded delay after attach; assert host exits/reports cleanly (no hang past a timeout), no orphans, no stale dirs, cleanup classification recorded.
2. `chaos_sigkill_capsule` — `docker exec <container> kill -9 1` (capsule is PID 1); assert reattach-or-clean-failure: a follow-up attach attempt either succeeds or fails with a clean error (never hangs), then invariants.
3. `chaos_drop_control_socket` — remove/disconnect the control socket the host↔capsule attach uses (read `common.rs`/the attach path for the socket's location inside the container; `docker exec <container> rm <socket>` or network-disconnect if socket removal is not meaningful — pick from what the harness exposes and record the choice); assert the host surfaces a clean attach failure and invariants hold.

Time-bound every wait (the harness's existing wait helpers) — a chaos test that can hang is worse than none. Replaying: same seed → same schedule; document `JACKIN_CHAOS_SEED=<n> cargo nextest run … -E 'test(chaos_kill_container_mid_session)'` in TESTING.md.

**Verify**: `cargo nextest run -p jackin --features e2e --profile docker-e2e -E 'test(chaos)'` locally with Docker running → 3 pass, seeds printed. If an invariant genuinely fails (a real orphan/stale-dir bug): STOP per below.

### Step 3: Scheduled lane

Append a `dind-chaos` job to `hygiene.yml` mirroring `scheduled-hygiene`'s setup steps (checkout, rustup cache, mise-action with `install_args: "cargo-binstall rust cargo:cargo-nextest"`, registry cache, rust-cache with its own `shared-key`), plus Docker availability (ubuntu runners ship Docker), then: build/export the local capsule the way `cargo xtask ci --e2e` does (read `crates/jackin-xtask/src/ci.rs`'s e2e arm and invoke the same commands, or simply run `cargo xtask ci --e2e`-equivalent filtered to chaos: `cargo nextest run -p jackin --features e2e --profile docker-e2e -E 'test(chaos)'` after the capsule export step). `continue-on-error: false` but the job is schedule-only — it can never block a PR. Seed: let it default (varying by date is NOT allowed — deterministic default; variety comes from `workflow_dispatch` with a seed input: add `workflow_dispatch.inputs.chaos_seed` mapped to `JACKIN_CHAOS_SEED` job env).

**Verify**: `gh workflow run Hygiene --ref <branch>` → the `dind-chaos` job runs green (or reports a real invariant failure — STOP case); paste the run URL in the PR body.

### Step 4: Docs

TESTING.md: chaos lane row (what it does, replay instructions). Roadmap Phase 3 sim item 4 → shipped note (turmoil half still sequenced).

**Verify**: `cargo xtask docs repo-links && cargo xtask roadmap audit` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

The three scenarios ARE the tests. Regression: the existing e2e suite must stay green (`cargo xtask ci --e2e` locally once, before the PR is marked ready — the chaos helpers must not disturb shared fixtures).

## Done criteria

- [ ] `chaos.rs` helper module; 3 seeded scenarios; every scenario prints its seed first
- [ ] All waits time-bounded; replay via `JACKIN_CHAOS_SEED` documented in TESTING.md
- [ ] `dind-chaos` hygiene job green on a dispatch run (URL in PR body)
- [ ] Existing e2e suite unaffected
- [ ] Roadmap + TESTING.md updated; `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- Any invariant fails against current `main` behavior (orphaned container, stale state dir, hang, unclassified cleanup) — that is a REAL BUG this plan exists to catch; report the seed + scenario + observed state; do not fix production code and do not weaken the assertion.
- The control socket's location/mechanism is not discoverable from the harness/attach code (Step 2.3) — report what you found instead of guessing a path.
- The harness cannot express a fault without modifying production code.
- Local Docker is unavailable AND the dispatch run cannot be triggered — report verification as blocked.

## Maintenance notes

- When plan 017's defect ledger lands, chaos-lane failures feed it (each failure = symptom + seed + the gate that caught it).
- The turmoil/proptest sim track (still sequenced behind daemon ports) will subsume the *in-process* interleaving coverage; this lane keeps owning the real-Docker end state.
- Reviewers: check every `docker` invocation in chaos.rs is scoped by the harness's label/name prefix — a chaos test that kills non-jackin containers on a shared runner is unacceptable.
