# Plan 012: Domain metrics — counters/histograms for the signals that should never be log rows, plus turso connection reuse

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-diagnostics/src/observability.rs crates/jackin-usage/src/telemetry_store.rs crates/jackin-usage/src/token_monitor`
> Verify the `init_metrics` excerpt below still matches; STOP otherwise.

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: MED
- **Depends on**: plans/002-export-filter-allowlist.md (storm gone first), plans/005-redaction-boundary.md (payload sites now local-only), plans/008-telemetry-level-and-categories.md (TRACE tier exists)
- **Category**: perf / direction
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

jackin❯ emits **zero domain metrics** — the only instruments are five process/runtime gauges (CPU, RSS, three tokio gauges) on a hardcoded 5 s cadence from host *and* every capsule (that cadence alone produced a 118.6 MiB metrics spool on a live backend). Meanwhile high-frequency facts that operators actually ask about — terminal throughput, frame rate, docker call counts, error counts by type, usage-refresh outcomes — exist only as (post-plan-005 local-only) debug log lines, i.e. not queryable at all in the backend. The right shape per the OTel model: high-frequency observations become counters/histograms with low-cardinality labels; log rows stay for discrete facts. Bonus in the same subsystem: the usage/token-monitor DB layer reopens a turso database + connection on every write/poll, which is both latency and (pre-plan-002) span-noise.

## Current state

- Instrument construction — `crates/jackin-diagnostics/src/observability.rs:1017-1118` (verified firsthand): `init_metrics(resource, endpoint, app_handle)` builds the `SdkMeterProvider` with `PeriodicReader … with_interval(Duration::from_secs(5))`, `meter("jackin")`, then the five gauges (`process.cpu.utilization`, `process.memory.usage`, `tokio.runtime.workers`, `tokio.runtime.alive.tasks`, `tokio.runtime.global.queue.depth`). Called from host `init` (`:778-786`, only when a metrics endpoint resolves) and capsule `init_capsule` (`:875`, unconditional `.ok()`).
- No other meter/counter/histogram exists in the workspace (audited; grep to reconfirm: `rg -n "u64_counter|f64_histogram|\.meter\(" crates/`).
- Candidate high-frequency sources (post-plan-005 these are `cdebug_local!`/`ctrace!` sites): PTY chunk metrics `session.rs:1147`, frame counters `client_writer.rs:131`, compositor frame sites, mouse sites; docker calls go through `docker_client.rs` + `shell_runner.rs` (plan 007 added `subprocess_done` events with `elapsed_ms`/`exit_code`).
- Error counts: plan 003 made failures ERROR-severity with `error_type`.
- turso per-call opens — `crates/jackin-usage/src/telemetry_store.rs:97-103` (`Builder::new_local(path).build().await` + `db.connect()` per `store_usage_snapshots`; schema-init cached per path at `:107-115`) and `token_monitor/opencode.rs:14-24` (fresh build+connect per poll). Workspace DB rule: turso only (ENGINEERING.md:20).
- `tracing-opentelemetry` 0.33 ships `MetricsLayer`: tracing events with `monotonic_counter.*` / `counter.*` / `histogram.*` / `gauge.*` field prefixes become OTel instruments; other fields become attributes. This lets hot paths emit metrics through the existing `tracing` macros without threading a `Meter` handle everywhere.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt/clippy | `cargo fmt --check` ; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Tests | `cargo nextest run --all-features` | pass |

## Scope

**In scope**: `crates/jackin-diagnostics/src/observability.rs` (MetricsLayer wiring, interval config), the metric emit sites listed in Step 2 (capsule session/client_writer/compositor, ShellRunner tail, error path), `crates/jackin-usage/src/telemetry_store.rs` + `token_monitor/opencode.rs` (connection reuse), tests, `docs/content/docs/reference/runtime/diagnostics.mdx` (metrics catalog).

**Out of scope**: dashboards/queries (backend concern); changing the five existing gauges' semantics; any new label with unbounded cardinality (container names, run ids as metric LABELS — the resource already carries run id); usage-refresh lock-window restructuring (flagged in audit as PERF-05 — needs its own careful plan; record in README rejected/deferred list).

## Git workflow

- Propose branch `feat/domain-metrics`; operator confirm; `git commit -s` per step; push each.

## Steps

### Step 1: Wire MetricsLayer + configurable cadence

1. In `observability.rs::init` and `init_capsule`, add `tracing_opentelemetry::MetricsLayer::new(meter_provider.clone())` as a fourth layer, filtered to a dedicated target: give it an `EnvFilter` allowing only `target: "jackin_metrics"` events (so ordinary logs never hit the metrics layer and metric events never hit the log bridge — add `jackin_metrics=off` awareness to the plan-002 directive? Simpler: metric events use `target: "jackin_metrics"` which is NOT in `EXPORT_TARGETS`, so the log/span layers ignore them already; the MetricsLayer gets its own `EnvFilter::new("jackin_metrics=trace")`).
2. Respect the standard `OTEL_METRIC_EXPORT_INTERVAL` env (milliseconds, per OTel spec) for the PeriodicReader interval; default stays 5 s for the host but becomes **60 s for the capsule** (each container is a permanent extra stream; comment why). Read once in `init_metrics` via a parameter.
3. Capsule metrics stop being unconditional: only when the endpoint resolves (mirror host semantics — `init_capsule` currently `.ok()`s a build against the same endpoint; keep, but interval per (2)).

**Verify**: seam-style unit: MetricsLayer present in composition compiles + a `tracing::info!(target: "jackin_metrics", monotonic_counter.jackin_test = 1u64)` under a test provider with an in-memory `ManualReader`/`InMemoryMetricExporter` (opentelemetry_sdk testing feature — same dev-dep as plan 001) yields the instrument. If the in-memory metrics exporter is unavailable in 0.32's testing module, verify by construction + a smoke assertion that the event is consumed without error, and note it.

### Step 2: Emit the catalog

Small helper macro in `jackin-diagnostics` (exported) to keep call sites terse and the target single-sourced:

```rust
#[macro_export]
macro_rules! metric {
    ($($fields:tt)*) => { tracing::trace!(target: "jackin_metrics", $($fields)*) };
}
```

(trace level: cheapest to filter; MetricsLayer's filter admits it.) Catalog + sites:

| Metric | Type prefix | Site |
|---|---|---|
| `jackin.terminal.pty_bytes` | `monotonic_counter.` | `session.rs` feed_pty (beside the `:1147` metrics cdebug) |
| `jackin.terminal.frames` | `monotonic_counter.` | `client_writer.rs:131` region |
| `jackin.render.frame_duration_us` | `histogram.` | compositor frame site (`daemon/compositor.rs:151` region — it already computes geometry/timing) |
| `jackin.input.mouse_events` | `monotonic_counter.` | `daemon/mouse_input.rs` dispatch entry |
| `jackin.subprocess.duration_ms` | `histogram.` (attr `program`) | plan-007 `subprocess_done` helper in `run.rs` |
| `jackin.errors` | `monotonic_counter.` (attr `error_type`) | `emit_jsonl_error` body (`observability.rs`) |
| `jackin.usage.refresh_duration_ms` | `histogram.` (attr `outcome`) | around `refresh_active_account_snapshots` call site (`jackin-usage/src/usage.rs` — the `#[tracing::instrument]` fn at `:377`; emit at its end) |

Label discipline (comment at the macro): attributes must be low-cardinality (program name, error type, outcome — never container/run ids/paths).

**Verify**: `cargo nextest run --all-features` green; grep each site exists: `rg -n "jackin_metrics|metric!\(" crates/ | wc -l` ≥ 8.

### Step 3: turso connection reuse

1. `telemetry_store.rs`: cache the `(Database, Connection)` per path alongside the existing `INITIALIZED_DBS` set (`:107-115`) — a `OnceLock<Mutex<HashMap<PathBuf, turso::Connection>>>` (check `Connection: Clone`/Send in the pinned turso 0.7.0-pre; if not Clone, store and lock around use). Reuse in `store_usage_snapshots` (`:97-103`).
2. `token_monitor/opencode.rs:14-24`: same cache (share the helper — put `fn cached_connection(path) -> anyhow::Result<turso::Connection>` in `telemetry_store.rs` and call from both; DRY rule).

**Verify**: `cargo nextest run -p jackin-usage --all-features` — existing store tests pass; add `connection_is_reused` (two writes, one underlying open — assert via the cache map length or a counter in the helper).

### Step 4: Docs + gate

diagnostics.mdx: metrics catalog table (name, type, labels, source) + `OTEL_METRIC_EXPORT_INTERVAL` + capsule 60 s default. environment-variables.mdx telemetry subsection gains the interval var (coordinate with 008/013 edits).

**Verify**: fmt/clippy/`cargo nextest run --all-features` exit 0; `cargo xtask docs repo-links` exit 0.

## Test plan

Step 1's layer test; Step 3's reuse test; pure tests for any interval-parse helper (ms env → Duration, garbage → default). Metric emit sites are fire-and-forget macros — coverage via compilation + the layer test; do not attempt per-site assertion.

## Done criteria

- [ ] MetricsLayer wired host+capsule; interval env honored; capsule default 60 s (code + doc)
- [ ] 7 catalog metrics emitting via `metric!` (grep count)
- [ ] `errors` counter increments on the error path (layer test or code inspection noted in PR)
- [ ] turso: no `Builder::new_local` outside the cached helper (`rg -n "new_local" crates/jackin-usage/src` → 1 hit)
- [ ] fmt/clippy/nextest green; diagnostics.mdx catalog added; `plans/README.md` updated

## STOP conditions

- `MetricsLayer` in tracing-opentelemetry 0.33 is incompatible with the 0.32 SDK meter provider (version pairing — check Cargo.lock pairing rules in the crates' READMEs). If incompatible, STOP and propose either version bumps or direct `Meter` plumbing for the two host-side metrics only.
- turso `Connection` is not reusable across await points in the store's runtime model (`telemetry_store.rs:78-88` has its own current-thread runtime) — report; do not fork a second DB stack (ENGINEERING.md hard rule).
- Any metric label would need container name/run id to be useful — stop and reconsider that metric (resource attrs already scope by run).

## Maintenance notes

- Usage-refresh flock window (audit PERF-05: lock held across DB write + fsync'd JSON materialization) deliberately not touched — cross-container coordination semantics need their own plan.
- Reviewer: cardinality — reject labels with user data. Watch the metrics spool size on a live backend after a week; tune capsule interval if still heavy.
- Deferred: dropped-OTLP-records counter (needs SDK observability hooks; revisit after plan 011's queue decision).
