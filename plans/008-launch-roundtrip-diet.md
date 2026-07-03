# Plan 008: Launch round-trip diet — dedupe Docker/list calls, probes, double work at startup

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index. Steps here are independent — land what verifies, skip
> (and report) what doesn't.
>
> **Drift check (run first)**: `git diff --stat a2ec1b23..HEAD -- crates/jackin-host/src/caffeinate.rs crates/jackin/src/preflight.rs crates/jackin/src/app.rs crates/jackin-runtime/src/runtime/identity.rs crates/jackin-runtime/src/spin_wait.rs crates/jackin-runtime/src/runtime/launch/restore_resolve.rs crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs`
> On mismatch with an excerpt, STOP for that step only.

## Status

- **Priority**: P3
- **Effort**: M (aggregate of S-steps)
- **Risk**: LOW
- **Depends on**: none; safest after 003 (same pipeline file in one step)
- **Category**: perf
- **Planned at**: commit `a2ec1b23`, 2026-07-03

## Why this matters

Beyond the headline stalls (plans 001–007), every launch pays a tax of small
serial round-trips: three keep-awake container-list calls (even when no
workspace uses `keep_awake`), an unfiltered preflight `list_containers` that
duplicates the connect that follows, a doubled restore-candidate scan that
docker-inspects every prior instance twice, a doubled diagnostics-dir prune,
two serial `git config` spawns, and fixed 500 ms/1 s poll intervals that
overshoot readiness. Individually 50–500 ms; together ~1–3 s of every launch,
and they multiply under a loaded Docker Desktop VM boundary.

## Current state (per step)

- **(a) keep_awake**: `crates/jackin-host/src/caffeinate.rs` —
  `reconcile_inner` always calls `count_keep_awake_agents` →
  `docker.list_containers(&[LABEL_MANAGED, LABEL_KEEP_AWAKE], false)`
  (lines ~114 and ~214-220). Called 3× per load:
  `crates/jackin/src/app/load_cmd.rs:105` (pre), `:132` (post), and
  `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs:1000` (mid,
  required — see its comment at 997–999). No config gate; callers get
  `(paths, docker, runner)` only.
- **(b) preflight**: `crates/jackin/src/preflight.rs:202-218` —
  `check_docker_daemon` connects and runs `list_containers(&[], false)`
  (unfiltered, all containers) as a reachability probe;
  `load_cmd.rs:49-50` then calls `connect_docker()` again.
- **(c) restore double-scan**:
  `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs:264-339` early
  gate runs `resolve_current_restore_candidate_timed` (index scan + one
  `inspect_container_state` per candidate), result discarded when `None`;
  line ~556 `resolve_restore_candidate` runs the same resolver again
  (`restore_resolve.rs:43`). Constraint: the early call may run before agent
  resolution (unselected variant, all agents), the later one is agent-filtered
  — reuse is only valid when the early call's agent matches the resolved one.
- **(d) diagnostics double-prune**: `crates/jackin/src/app.rs:125`
  `jackin_diagnostics::prune_old_runs(&paths)` — and `RunDiagnostics::start`
  already prunes when persisting (`jackin-diagnostics/src/run.rs:157`).
- **(e) identity**: `crates/jackin-runtime/src/runtime/identity.rs:91-118` —
  two sequential `git config user.name` / `user.email` captures.
- **(f) poll granularity**: `crates/jackin-runtime/src/spin_wait.rs:59`
  `let spins = interval.as_millis() as u64 / SPIN_MS;` (SPIN_MS=80) — a
  sub-80 ms interval yields 0 sleeps (busy-loop latent bug);
  `wait_for_capsule_daemon` fixed 500 ms interval
  (`runtime/attach.rs:116-117`), `wait_for_dind` fixed 1 s
  (`attach.rs:1088-1089` per audit).
- **(g) config double-load (console)**: `crates/jackin/src/app.rs:135`
  loads config for the console; `load_cmd.rs:206` reloads after console
  returns (full re-parse + validation).

## Commands you will need

| Purpose   | Command                                                                    | Expected on success |
|-----------|----------------------------------------------------------------------------|---------------------|
| Format    | `cargo fmt --check`                                                        | exit 0              |
| Lint      | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0        |
| Typecheck | `cargo check --all-targets`                                                | exit 0              |
| Tests     | `cargo nextest run -p jackin -p jackin-runtime -p jackin-host -p jackin-diagnostics` | all pass  |
| Full      | `cargo nextest run --all-features`                                         | all pass            |

## Scope

**In scope**: the files listed per step above, plus their `tests.rs` siblings,
plus `docs/content/docs/guides/workspaces.mdx` only if the keep_awake gate
changes operator-visible behavior (it does not — reconcile outcomes are
identical; skip unless a reviewer asks).

**Out of scope**: pipeline stage reordering (003), image logic (001/006),
console `docker ps` subprocess refresh loop (recorded deferred — it is a
console-refresh cost, not launch), snapshot exec poll loop (off launch path).

## Git workflow

- Branch: `git checkout -b perf/launch-roundtrip-diet` off `main`.
- One `git commit -s` per lettered step (`perf(host): …`, `perf(preflight): …`);
  push after each per CONTRIBUTING.md unless dispatch forbids it.

## Steps

### Step a: Gate + share keep_awake reconciliation

1. Add an `any_keep_awake_enabled: bool` parameter (or a small context struct)
   to `jackin_host::caffeinate::reconcile`, computed by callers from config
   (`config.workspaces.values().any(|w| w.keep_awake…)` — find the exact field
   via `grep -rn keep_awake crates/jackin-config/src`). When false AND no
   caffeinate pid file exists (`read_pid_file` — cheap stat, keeps the
   "operator disabled it while caffeinate runs" teardown working), return
   before `count_keep_awake_agents`.
2. Keep all three call sites (pre/mid/post have real semantics); the gate makes
   each a no-op stat for non-users. Do not attempt snapshot-sharing across the
   three calls (state changes between them by design).

**Verify**: `cargo nextest run -p jackin-host` all pass; new unit test: gate
false + no pidfile → zero DockerApi calls (mock).

### Step b: Cheapen the preflight daemon probe

In `preflight.rs:check_docker_daemon`, replace `list_containers(&[], false)`
with the cheapest bollard liveness call available on the client (`ping` —
check `jackin-docker/src/docker_client.rs` for an existing wrapper; add a thin
`ping()` to the `DockerApi` trait + bollard impl if absent, mirroring
`list_containers`'s error mapping). Keep the E001 error mapping identical.

**Verify**: `cargo nextest run -p jackin` all pass (preflight tests assert
messages, not the probe call — update mocks if the trait grew `ping`).

### Step c: Reuse the early restore resolution

In `launch_pipeline.rs`: capture the early gate's result
(lines 264–339) as `early_restore: Option<(AgentContext, Resolution)>` — i.e.
record which agent (or "unselected") it was computed for. At the later
`resolve_restore_candidate` call (~line 556), when the resolved agent equals
the early context (or the early scan returned a definitive `None` for the
unselected-all-agents variant, which subsumes any specific agent), skip the
re-scan and feed the cached value through to the
`related_restore_candidates` handling. If the equivalence rule cannot be
established from `restore_resolve.rs` reading (the unselected variant's None ⊇
per-agent None must hold — verify by reading
`resolve_unselected_current_restore_candidate_timed`), skip this step and
report.

**Verify**: `cargo nextest run -p jackin-runtime` all pass (restore tests are
extensive — they are the safety net here).

### Step d: Single diagnostics prune

Delete the `prune_old_runs` call at `app.rs:125` (the `RunDiagnostics::start`
prune already covers it). Confirm non-persisting paths (no run file created)
don't rely on the app-level prune — grep `prune_old_runs` callers; if the
start-side prune only fires when persisting, keep the app-level call but make
it `tokio::task::spawn_blocking` fire-and-forget instead of synchronous.

**Verify**: `cargo nextest run -p jackin-diagnostics -p jackin` all pass.

### Step e: One subprocess for git identity

Replace the two captures in `identity.rs:load_git_identity` with one:
`git config --get-regexp '^user\.(name|email)$'`, parsed line-wise (missing
keys → empty strings, exit code 1 with empty output means no matches — treat
as both-missing, not error). Keep both timing spans (start/stop around the
single capture, or collapse to one span `git_identity` and update any test
asserting the old span names — grep `git_user_name` in tests first).

**Verify**: `cargo nextest run -p jackin-runtime` all pass.

### Step f: Ramped readiness polls + spin_wait floor

1. `spin_wait.rs`: floor the sleep loop — `let spins = (interval.as_millis() as u64 / SPIN_MS).max(1);`
   so sub-80 ms intervals still yield.
2. Add an optional ramp: extend `spin_wait` with a variant (or a
   `next_interval` closure param) implementing 100 ms → ×2 → cap. Convert
   `wait_for_capsule_daemon` (`attach.rs:116-117`) to 100 ms start / 500 ms cap
   (same 30 s budget), and `wait_for_dind` to 200 ms start / 1 s cap (same
   total budget). Keep both functions' external contracts (message, budget,
   error text) unchanged.

**Verify**: `cargo nextest run -p jackin-runtime` all pass; new unit test for
the ramp sequence and the sub-80 ms floor.

### Step g: Console config single-load

In `load_cmd.rs:206` region: reload the config after the console **only if**
the console reports it persisted changes (the console's save path goes through
`ConfigEditor.save` — find the seam: if `run_console`/console entry returns or
exposes a "config dirty" signal, use it; if not, have the console return the
updated `AppConfig` it already holds). If neither is reachable without console
API surgery beyond these files, skip and report.

**Verify**: `cargo nextest run -p jackin` all pass.

### Final gates

`cargo fmt --check` + clippy gate + `cargo nextest run --all-features` → green.

## Test plan

Per-step verifies above; each step needs at least one new/updated unit test in
the owning crate's existing `tests.rs`, following that file's mock patterns
(DockerApi call-recording mocks exist in jackin-runtime and jackin-host tests).

## Done criteria

- [ ] keep_awake: zero container-list calls when feature unused (test)
- [ ] preflight: no unfiltered `list_containers` probe remains (grep)
- [ ] restore: single candidate scan on the common path (test or skip-report)
- [ ] diagnostics: single prune per process (grep call sites)
- [ ] identity: single git subprocess (test asserts one capture)
- [ ] spin_wait floor + ramps in both waiters (tests)
- [ ] fmt/clippy/`cargo nextest run --all-features` green
- [ ] Only in-scope files modified (`git status`); README row updated,
      including which lettered steps were skipped and why

## STOP conditions

- Any step's stated seam doesn't exist as described (per-step skip + report;
  do not invent new cross-crate APIs beyond the thin `ping()` in step b).
- Restore-equivalence rule (step c) not provable from the resolver code.

## Maintenance notes

- Step f's ramp interacts with capsule-boot changes (audit found the capsule
  socket usually ready in <100 ms once `docker run` returns — the ramp is what
  converts that into visible latency reduction).
- Reviewer focus: step a's pidfile-still-running teardown path; step c's
  agent-equivalence reasoning.
- Deferred (README): console `docker ps` subprocess refresh; bollard client
  reuse across preflight/load (audit PERF-06, small); `Multiplexer::new`
  git/gh probes off the socket-ready path (in-capsule; audit capsule PERF-02);
  post-run firewall/sudo execs via bollard + overlap with socket wait (audit
  capsule PERF-03/DinD PERF-05).
