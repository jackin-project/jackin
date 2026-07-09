# Plan 042: High-frequency internals become metrics — instrument set for terminal/render/input/usage hot paths

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat fabe88406..HEAD -- crates/jackin-diagnostics/src/observability.rs crates/jackin-capsule/src/client_writer.rs crates/jackin-capsule/src/daemon/compositor.rs crates/jackin-capsule/src/session.rs crates/jackin-launch-tui/src/tui/subscriptions.rs crates/jackin-usage/src/usage.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition. Plans 018/041 landing IS expected
> drift — read their diffs and continue; anything else is a real STOP.

## Status

- **Priority**: P1
- **Effort**: M-L
- **Risk**: MED (capsule hot paths; every change is additive counters + tier demotion, no control-flow change)
- **Depends on**: plans/code-health/018-telemetry-drift-proofing.md (metric-name registry). Soft: 041 (its `operation_metric` becomes a thin wrapper over this plan's instruments; either order compiles — whichever lands second reconciles).
- **Category**: perf / tech-debt
- **Planned at**: commit `fabe88406`, 2026-07-09

## Why this matters

The live-backend audit measured 242,689 PTY/render debug rows and 21,611 mouse-move rows in one store — high-frequency internals exported as individual DEBUG log records, drowning lifecycle events and inflating the spool to 1.65 GiB. The roadmap (Phase 8 item 6) prescribes the fix: "terminal bytes, render/frame durations, painted cells, mouse-event counts, and DB statement counts become counters/histograms instead of log rows, from both host and capsule." The metric infrastructure already ships (OTLP `PeriodicReader`, process/tokio gauges); what is missing is the instrument set and the conversion of each emit site from per-event row to counter increment, with the raw row demoted to the TRACE tier that already exists capsule-side.

## Current state

All excerpts verified by direct read at `fabe88406`.

- Metric infra shipped: `crates/jackin-diagnostics/src/observability.rs:1043-1152` — `init_metrics` builds `SdkMeterProvider` + `PeriodicReader` (5s) and registers `process.cpu.utilization` (:1080), `process.memory.usage` (:1098), `tokio.runtime.workers` (:1116), `tokio.runtime.alive.tasks` (:1126), `tokio.runtime.global.queue.depth` (:1135), `jackin.diagnostics.events` (:1146). All names are inline literals today; plan 018 Step 2 moves them into the registry — this plan's new instruments mint from that registry.
- The capsule also gets a meter: `init_capsule` calls `init_metrics` at `observability.rs:799`. Both processes can therefore record instruments; what is missing is an accessor for increment sites (the provider handle lives in the private `PROVIDERS` OnceLock, `observability.rs:732/827`).
- Emit sites to convert (each currently a per-event debug row):
  - `crates/jackin-capsule/src/client_writer.rs:126-149` — `log_emission`: computes `EmittedFrameMetrics` (struct :171-183: bytes, cursor_moves, sgr_resets, osc8 opens/closes, painted_cells, full_frame_repaint, …) then emits one `cdebug!("send: bytes={} cursor_moves={} … painted_cells={} …")` per frame; raw dump already TRACE-gated (`ctrace_payload!` :147).
  - `crates/jackin-capsule/src/daemon/compositor.rs:83-93` — per-frame `cdebug!("render: reason={} … duration_us={} …", started.elapsed().as_micros(), …)`; sibling rows at :95 (`render_alloc`), :170/:179 (loop internals), :375 (`pane scroll frame`), :400 (`frame-geom`).
  - `crates/jackin-capsule/src/session.rs:1101-1111` — `feed_pty` already demoted its byte dump to `ctrace_payload!` (:1105); the *count* (bytes received) is not recorded anywhere.
  - `crates/jackin-launch-tui/src/tui/subscriptions.rs:493` — `"cockpit-dialog-mouse"` per-move debug rows (the dossier's 21,611-row family), host side.
  - `crates/jackin-usage/src/usage.rs:380` — `otel.name = "usage:refresh_accounts"` span exists; the dossier's ask is `usage.accounts_refreshed` as a counter beside it.
  - Error counter: `RunDiagnostics::error_typed` (`crates/jackin-diagnostics/src/run.rs:391`) is the typed-error choke point — one increment there covers `jackin.errors.count` by `error.type`.
- Capsule TRACE tier exists: `ctrace_payload!` (`crates/jackin-usage/src/logging.rs:224-235`) gates on `trace_enabled()` (`JACKIN_TELEMETRY_LEVEL=trace`), exporting at TRACE when OTLP is active, file-only otherwise. Host side has no TRACE emit helper — host firehose rows stay `debug_log!` and are NOT demoted this plan (that is plan 043's per-sink filter job).
- Conventions: capsule AGENTS — "No blocking on the render/control path"; the increments must be lock-free atomic instrument calls (OTel counters are). jackin-usage AGENTS — do not introduce a parallel logging path (instruments are the metrics tier, not a log path). Tests in sibling `tests.rs` only. dhat allocation budgets exist for render paths (`crates/jackin-capsule/tests/render_allocation.rs` asserts `blocks <= 3`/`bytes <= 1024` per frame scope) — instrument recording must not allocate per frame on that path (pre-build attribute arrays once).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Diagnostics tests | `cargo nextest run -p jackin-diagnostics` | all pass |
| Capsule tests (incl. allocation budgets) | `cargo nextest run -p jackin-capsule` | all pass |
| Usage + launch-tui tests | `cargo nextest run -p jackin-usage -p jackin-launch-tui` | all pass |
| Workspace clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-diagnostics/src/observability.rs` — instrument definitions in `init_metrics` + a public increment API (new small module `crates/jackin-diagnostics/src/metrics.rs` + `metrics/tests.rs` preferred, keeping `observability.rs` from growing)
- `crates/jackin-capsule/src/client_writer.rs`, `crates/jackin-capsule/src/daemon/compositor.rs`, `crates/jackin-capsule/src/session.rs` (+ their `tests.rs`)
- `crates/jackin-launch-tui/src/tui/subscriptions.rs` (+ tests)
- `crates/jackin-usage/src/usage.rs` (+ tests)
- `crates/jackin-diagnostics/src/run.rs` (one increment in `error_typed`)
- `crates/jackin-diagnostics/README.md` (structure table row for `metrics.rs`)
- Roadmap Phase 8 item 6 status note

**Out of scope** (do NOT touch):
- `jackin.db.statement_count` — the `emit_insn` storm is storage-engine-internal (turso) with no clean jackin❯-side hook; record as a deferred residual in the PR body (the dossier's Phase 3 item 5 covers dropping dependency-internal spans, plan 043's filters handle it).
- `docker.inspect_count` — belongs to the Docker-lifecycle facade-adoption wave (041 maintenance note), not here.
- Per-sink filters, `JACKIN_DEBUG` retirement (plan 043); the conformance lane (plan 044).
- Any change to render/input control flow, frame pacing, or the `EmittedFrameMetrics` scanner itself.

## Git workflow

- Branch off `main`: `feat/telemetry-hot-path-metrics`.
- Conventional Commits (`feat(telemetry): …` / `perf(capsule): …`), sign `-s`, push per commit. PR to `main`; do not merge.
- Capsule dependency closure touched → capsule smoke block in the PR body, verbatim from `.github/PULL_REQUEST_TEMPLATE.md`.

## Steps

### Step 1: Instrument definitions + increment API

Create `crates/jackin-diagnostics/src/metrics.rs` (+ declare in lib.rs, re-export the increment fns). Define, minted from the (post-018) registry consts — add the new names there first:

| Instrument | Kind | Unit |
|---|---|---|
| `jackin.terminal.bytes_sent` | u64 counter | By |
| `jackin.terminal.bytes_received` | u64 counter | By |
| `jackin.terminal.cursor_moves` | u64 counter | 1 |
| `jackin.render.duration` | u64 histogram | us |
| `jackin.render.painted_cells` | u64 counter | 1 |
| `jackin.render.frames` | u64 counter | 1 |
| `jackin.input.mouse_events` | u64 counter | 1 |
| `jackin.usage.accounts_refreshed` | u64 counter | 1 |
| `jackin.errors.count` | u64 counter (attr: `error.type`) | 1 |

Implementation shape: a `struct HotPathMetrics { … }` of instrument handles built once inside `init_metrics` (both host and capsule call it) and stored in a `OnceLock<HotPathMetrics>`; public fns like `pub fn record_render(duration_us: u64, painted_cells: u64, bytes: u64, cursor_moves: u64)` and `pub fn incr_mouse_events()` that no-op when the OnceLock is empty (no OTLP → zero cost beyond one atomic load). No per-call allocation: attribute-less instruments except `jackin.errors.count`, whose `error.type` KeyValue is built from the `&'static`-leaning strings already used by `error_typed`.

**Verify**: `cargo nextest run -p jackin-diagnostics` → pass; new unit test in `metrics/tests.rs` builds a `SdkMeterProvider` with an in-memory/manual reader, installs the handles, records, and asserts the counter sums (use `opentelemetry_sdk::metrics::InMemoryMetricExporter` if present in the pinned version — check `cargo doc`/docs.rs for the exact name; a `ManualReader` collect works too).

### Step 2: Capsule render/writer sites

1. `client_writer.rs::log_emission` (:126-149): always call `jackin_diagnostics::metrics::record_frame(metrics.bytes as u64, metrics.cursor_moves as u64, metrics.painted_cells as u64)` (compute `scan_emitted_frame` result once — note it currently only runs under `debug_enabled()`; keep the scan debug-gated and record only `bytes.len()` unconditionally if the scan cost is the reason for the gate — decide by reading the scanner's cost: it is a single linear pass (:185+), so running it always is acceptable ONLY if the render-allocation tests stay green; otherwise unconditional bytes + debug-gated detail counters, and say which branch you took in the PR body). Demote the `cdebug!("send: …")` row to `ctrace_payload!` (the per-frame text row is the firehose the metrics replace).
2. `compositor.rs` (:83-93): `record_render(started.elapsed().as_micros() as u64, …)` + increment `jackin.render.frames`; demote the `cdebug!("render: …")` row and the `:400` `frame-geom` row to `ctrace_payload!`. Leave `:95 render_alloc` (feeds the dhat budgets), `:170/:179` (self-gated loop internals) and both `clog!` failure lines untouched.
3. `session.rs::feed_pty` (:1101): add `metrics::incr_terminal_bytes_received(bytes.len() as u64)` before the existing `ctrace_payload!`.

**Verify**: `cargo nextest run -p jackin-capsule` → all pass, **including** `render_allocation` (the dhat budgets prove no new per-frame allocation); `grep -n 'cdebug!("send:' crates/jackin-capsule/src/client_writer.rs` → no match.

### Step 3: Host mouse + usage + error sites

1. `subscriptions.rs:493` area (`cockpit-dialog-mouse`): increment `jackin.input.mouse_events` per handled mouse event; wrap the existing per-move debug row emission so `Moved` rows only emit at the trace tier — host side has no `ctrace_payload!`, so gate on `jackin_diagnostics::telemetry_level(is_debug_mode()) == TelemetryLevel::Trace` (both are already public: `lib.rs:27-31`). Non-`Moved` kinds (clicks/drags) may stay debug-tier.
2. `usage.rs` around :380: increment `jackin.usage.accounts_refreshed` by the refreshed-account count where the `usage:refresh_accounts` span closes (find the natural count variable in that function; if none exists, count is the accounts iterated — read the function body first).
3. `run.rs::error_typed` (:391): increment `jackin.errors.count` with the `error.type` attribute.

**Verify**: `cargo nextest run -p jackin-usage -p jackin-launch-tui -p jackin-diagnostics` → all pass.

### Step 4: Prove the volume drop

Extend `metrics/tests.rs` (or `observability/otlp/tests.rs` if the rig lives there more naturally): with the test subscriber at debug level, simulate 100 frame emissions through the converted path — assert the in-memory log exporter captured **zero** `send:`/`render:` debug rows while the counters advanced. This is the executable form of the dossier acceptance check "DEBUG volume ≥10× lower".

**Verify**: `cargo nextest run -p jackin-diagnostics -p jackin-capsule` → pass.

### Step 5: Docs + gate

`crates/jackin-diagnostics/README.md` structure row for `metrics.rs`; roadmap Phase 8 item 6 → shipped-for-named-paths note (db-statement + docker-inspect residuals recorded). Full gate.

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → pass; `cargo xtask ci --fast` → `ci gate OK`; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0.

## Test plan

- New: instrument-recording unit test (Step 1), zero-debug-rows-under-load test (Step 4), plus per-site assertions where a suite already drives the path (capsule daemon tests drive `feed_pty`/render — add counter assertions only where a handle is reachable without contortion; the Step 4 volume test is the load-bearing one).
- Regression: full capsule suite including `render_allocation` dhat budgets; `jackin-launch-tui` + `jackin-usage` suites.

## Done criteria

- [ ] 9 instruments registered (8 named + errors.count), names in the registry, no inline metric-name literals added
- [ ] `client_writer`/`compositor` per-frame `cdebug!` rows demoted to trace tier; counters recorded unconditionally (or the documented debug-gated variant)
- [ ] `feed_pty` bytes, mouse events, accounts refreshed, typed errors counted
- [ ] Step 4 test: 100 simulated frames → 0 debug rows, counters advanced
- [ ] `render_allocation` dhat budgets green (no new per-frame allocation)
- [ ] Roadmap item 6 + README updated; `cargo xtask ci --fast` → `ci gate OK`
- [ ] `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- Plan 018's registry is absent (metric names would have to be inline literals — the exact drift 018 exists to prevent).
- The pinned `opentelemetry_sdk` exposes no test-usable metric reader/exporter (Step 1's assertion has no seam) — report the version and what it does expose.
- `render_allocation` budgets fail after Step 2 — the instrument path allocates on the render path; report the dhat delta instead of loosening the budget literals.
- Recording requires holding any lock across the render/control path (capsule AGENTS rule) — the OTel counter API should be lock-free; if the handle plumbing forces otherwise, stop.

## Maintenance notes

- Plan 043's per-sink filters make the trace-tier demotions operator-controllable per sink; plan 044's conformance lane asserts the volume contract permanently (its budget consts should reuse Step 4's scenario).
- `jackin.db.statement_count` + `docker.inspect_count` are the recorded residuals (storage-engine hook / Docker-lifecycle facade wave).
- Reviewer scrutiny: the debug-gate decision in Step 2.1 (scan cost vs unconditional counters) and that no demoted row was one an operator triage flow depends on at debug level (the trace tier + metrics must jointly cover what the row carried).
