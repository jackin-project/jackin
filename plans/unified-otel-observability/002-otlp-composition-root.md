# Plan 002: Rework the jackin-diagnostics composition root to the direct-OTLP runtime contract

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- crates/jackin-diagnostics Cargo.toml crates/jackin/Cargo.toml crates/jackin-capsule/Cargo.toml crates/jackin-usage/Cargo.toml`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED (touches every binary's telemetry init; behavior is runtime-gated so product flows stay working)
- **Depends on**: plans/unified-otel-observability/001-telemetry-schema-crate.md
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements "Direct OTLP runtime contract" and the Resource half of "Resource and correlation lifetimes"; the roadmap item is the binding contract and overrides this plan on any conflict.
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

The roadmap item pins an exact exporter/runtime contract: baseline (non-optional) OTLP/gRPC deps in all product binaries, three signals, gzip, TLS, pinned experimental gRPC retry isolated in `jackin-diagnostics`, `ParentBased(AlwaysOn)` sampler, `OTEL_SDK_DISABLED` support, strict env validation before providers start, bounded memory-only queues, one dedicated single-worker export runtime, an ordered graceful shutdown, and a true no-op fast path when no endpoint is configured. Today the OTel stack is behind an `otlp` cargo feature, the Resource is only `service.name="jackin"` + version for every process, there is no sampler/`OTEL_SDK_DISABLED`/retry/gzip handling, and the metric interval is 5 s. This plan makes `jackin-diagnostics` the single composition root implementing that contract, without yet changing what is emitted (bridges/facade follow in plans 003–004).

## Current state

All in `crates/jackin-diagnostics` (verified at planning commit):

- `Cargo.toml:20-42` — the `otlp` cargo feature gates ALL OTel deps (`dep:opentelemetry`, `dep:opentelemetry-otlp`, `dep:opentelemetry_sdk`, `dep:opentelemetry-appender-tracing`, `dep:tracing-opentelemetry`, `dep:sysinfo`, `dep:tokio`); `test-support = ["otlp", "opentelemetry_sdk/testing"]`. `opentelemetry-otlp` at line 67 enables `["grpc-tonic", "metrics"]` — **no `gzip-tonic`, no TLS features, no retry feature, `internal-logs` (a default feature) not disabled**. `opentelemetry_sdk` (line 78) uses the experimental async-runtime batch processors — the manifest comment at lines 68-77 explains why: both binaries run current-thread tokio mains, so a dedicated multi-thread telemetry runtime drives the exporters.
- `observability.rs:481-525` — `struct OtlpProviders { tracer, logger, meter: Option<…> }`, `static PROVIDERS: OnceLock<OtlpProviders>`, `flush_and_shutdown()` at `:499`. Dedicated runtime `otel_runtime()` at `:538` (multi-thread, 1 worker, thread name `jackin-otel`).
- `observability.rs:732-742` — Resource is only `service.name = "jackin"` + `service.version` for every process:

  ```rust
  fn build_resource() -> Resource {
      let attributes = vec![
          KeyValue::new(keys::SERVICE_NAME, "jackin"),
          KeyValue::new(keys::SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
      ];
      Resource::builder().with_attributes(attributes).build()
  }
  ```

- `observability.rs:583-728` — endpoint handling: `OTEL_EXPORTER_OTLP_ENDPOINT` + per-signal vars (`ENDPOINT_VARS` `:596`), grpc-only protocol guard (`ensure_grpc_protocol` `:702`, `PROTOCOL_VARS` `:691`), loopback→`host.docker.internal` container rewrite (`container_otlp` `:396`, `rewrite_endpoint_for_container` `:420`).
- `observability.rs:751-846` — `build_otlp_providers(...)` builds tonic Span/Log exporters + async-runtime batch processors inside the telemetry runtime's `enter()` guard (`:770`). Span limits 64 attrs / 32 per event (`:784-786`). **No sampler is configured anywhere** (SDK default applies). Metrics: `init_metrics` `:1222-1348`, `PeriodicReader … with_interval(Duration::from_secs(5))` at `:1236`.
- `observability.rs:221-330` — entry points `init_tracing(debug, run_id) -> anyhow::Result<bool>` (`:221`) and `init_capsule_tracing(session_id, run_id, traceparent)` (`:302`).
- Public API consumed by binaries today (keep signatures compiling or migrate all callers in this plan): `init_tracing`, `init_capsule_tracing`, `shutdown_capsule_tracing`, `configured_endpoint`, `configured_endpoint_summary`, `container_otlp`, `unsupported_otlp_protocol`, `backend_query_hint` (re-exported in `lib.rs:31-77`).
- Feature plumbing in dependents: `crates/jackin/Cargo.toml:88` has `default = ["otlp"]`; `jackin-capsule` and `jackin-usage` depend on `jackin-diagnostics` with `features = ["otlp"]`.
- `OTEL_SDK_DISABLED`, `OTEL_TRACES_SAMPLER`, gzip, and retry are referenced nowhere in the workspace today (`grep -rn "OTEL_SDK_DISABLED\|OTEL_TRACES_SAMPLER" crates/` → empty).
- E016 (`UnsupportedOtlpProtocol`) is the operator error for bad protocol config: `crates/jackin/src/error.rs:162,259`.

## The target contract (from the roadmap item, "Direct OTLP runtime contract")

1. Traces, logs, metrics + OTLP/gRPC exporter are **baseline dependencies** of all product binaries — the `otlp` cargo feature disappears.
2. `opentelemetry-otlp` with `default-features = false`, features: the three signals (`trace`, `logs`, `metrics`), `grpc-tonic`, `gzip-tonic`, project-standard TLS (`tls-roots` or the rustls-based equivalent available in 0.32 — pick the one that compiles against the locked tonic 0.14.6 and note it in the manifest comment), and `experimental-grpc-retry`. `internal-logs` disabled (set `default-features = false`; do not re-enable).
3. Retry construction isolated in one `jackin-diagnostics` module (`observability/retry.rs`) so a future stable API replaces the experimental surface in one place. Retry applies only to retryable transport/throttle failures; auth/config failures and OTLP partial success are not retried (the experimental feature's policy config — set max 3 attempts, total budget within the 5 s export timeout).
4. Endpoint resolution: `OTEL_EXPORTER_OTLP_ENDPOINT` is the normal endpoint; standard per-signal endpoints override; all three signals must resolve when telemetry is enabled; only `grpc` protocol accepted (existing E016 path); standard timeout/compression/TLS/header/Resource/service-name env config validated **before** providers start; `OTEL_SDK_DISABLED=true` disables all three providers; a conflicting `OTEL_TRACES_SAMPLER` env (anything other than unset, `parentbased_always_on`) fails initialization with a typed error.
5. Sampler: explicit `ParentBased(AlwaysOn)`.
6. Queues/batching: 4,096 log records; 2,048 spans; batch ≤ 512; 1 s schedule delay; 5 s total export-attempt timeout; ≤ 3 retries; 30 s metric interval; one dedicated single-worker export runtime; 5 s coordinated shutdown budget. Saturation drops telemetry (never blocks product work).
7. Resource (immutable, cloned to all providers): `service.namespace=jackin`; `service.name` ∈ {`jackin`, `jackin-role`, `jackin-daemon`, `jackin-capsule`} chosen by the binary; package `service.version`; random `service.instance.id` per process start; `process.pid`; `process.executable.name`; `app.mode` ∈ {`one_shot`, `interactive`, `daemon`, `capsule`}; `container.id` in the Capsule when available; plus the applicable standard `os.*` (`os.type`, `os.version` where cheaply known) and `process.runtime.*` fields per the contract's "applicable … `os.*`, and runtime fields" row. Invocation/session/job ids never go on the Resource.
8. No endpoint / SDK disabled ⇒ no-op instrumentation, **no exporter runtime, worker, socket, or telemetry artifact**; enabled-checks happen before body formatting.
9. Graceful shutdown, one owner: stop accepting work → cancel/time-bound background work → final events → end spans → force-flush all signals → shut down tracer, logger, then meter providers while runtime alive → stop runtime.
10. Typed health: active signals, outer export attempt/success/failure counts, facade rejection count, flush and shutdown results. No parsing of exporter text; no promise of SDK-internal queue-drop/retry/partial-success counters.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Check | `cargo check --workspace --all-targets --locked` | exit 0 |
| Crate tests | `cargo nextest run -p jackin-diagnostics --all-features --locked` | all pass |
| Workspace tests | `cargo nextest run --workspace --all-features --locked` | all pass |
| Feature matrix (this crate) | `cargo hack check -p jackin-diagnostics --feature-powerset --all-targets --locked` | exit 0 |
| Conformance filter | `cargo nextest run -p jackin-diagnostics -p jackin-capsule --all-features --locked -E 'test(/conformance/)'` | all pass |
| Lint | `cargo xtask ci --only lint` | exit 0 |

## Scope

**In scope:**
- `crates/jackin-diagnostics/Cargo.toml`, `src/observability.rs` (+ new submodules `observability/config.rs`, `observability/retry.rs`, `observability/health.rs`, each with sibling `tests.rs`), `src/lib.rs`
- Root `Cargo.toml` (move the OTel family pins into `[workspace.dependencies]`: `opentelemetry`, `opentelemetry-otlp`, `opentelemetry-appender-tracing`, `tracing-opentelemetry`, keeping versions 0.32/0.33), `Cargo.lock`
- Feature-flag removal ripple: `crates/jackin/Cargo.toml`, `crates/jackin-capsule/Cargo.toml`, `crates/jackin-usage/Cargo.toml`, and every `#[cfg(feature = "otlp")]` inside `crates/jackin-diagnostics` (find them: `grep -rn 'feature = "otlp"' crates/ --include='*.rs' --include='*.toml'`)
- Binary composition roots, only to pass the new service identity: `crates/jackin/src/app.rs` (host init), `crates/jackin-capsule/src/main.rs` / `crates/jackin-usage/src/telemetry.rs` (capsule init), `crates/jackin/src/app/daemon_cmd.rs` + `crates/jackin-runtime/src/host_daemon.rs` (daemon init)
- `ratchet.toml` only if the `observability.rs` file-size bound (currently 2052) is exceeded — prefer splitting into the new submodules instead of raising the bound

**Out of scope:**
- What is emitted (event names, span shapes, JSONL layer) — plans 003+. `JackinDiagnosticsLayer` and `run.rs` keep working unchanged.
- Protocol/env propagation changes (plan 006), identity semantics like `cli.invocation.id` (plan 007).
- Removing `parallax.run.id` or any legacy key (plan 013).

## Git workflow

- Branch: `feature/unified-otel-observability` — single branch, single PR for the whole roadmap item (plans 001–015 together); no per-plan branch or separate PR. Conventional Commits, e.g. `feat(diagnostics): pin direct otlp runtime contract in composition root`. Sign `-s`, push after every commit.

## Steps

### Step 1: De-feature the OTel stack

Remove the `otlp` feature: make the seven optional deps non-optional in `crates/jackin-diagnostics/Cargo.toml`; change `test-support` to `["opentelemetry_sdk/testing"]`. Move `opentelemetry`, `opentelemetry-otlp`, `opentelemetry-appender-tracing`, `tracing-opentelemetry` pins to root `[workspace.dependencies]` and reference them with `workspace = true` (keep the explanatory manifest comments — they document the current-thread-runtime constraint, which still holds). Update `opentelemetry-otlp` to `default-features = false, features = ["trace", "logs", "metrics", "grpc-tonic", "gzip-tonic", "<tls feature>", "experimental-grpc-retry"]` (confirm exact 0.32 feature names with `cargo add --dry-run` or the crate's Cargo.toml in `~/.cargo/registry`; the signals may be named `trace`/`logs`/`metrics` or on-by-default — verify with `cargo tree -p opentelemetry-otlp -e features`). Delete every `#[cfg(feature = "otlp")]` / `#[cfg(not(feature = "otlp"))]` branch in the crate, keeping the otlp-side code. Update dependents' `features = ["otlp"]` references and `crates/jackin/Cargo.toml:88` `default = ["otlp"]`.

**Verify**: `cargo check --workspace --all-targets --locked` → exit 0; `cargo hack check -p jackin-diagnostics --feature-powerset --all-targets --locked` → exit 0.

### Step 2: Env config validation module

New `observability/config.rs`: a pure `pub fn resolve_otlp_config(env: &impl Fn(&str) -> Option<String>) -> Result<Option<OtlpConfig>, OtlpConfigError>` (inject env lookup for testability) that:
- returns `Ok(None)` when no endpoint var set OR `OTEL_SDK_DISABLED=true` (case-insensitive `true`);
- resolves the three per-signal endpoints (per-signal var overrides base; all three must resolve — with a base set they always do);
- validates protocol vars are absent or `grpc` (map to the existing `unsupported_otlp_protocol()`/E016 path);
- reads standard `OTEL_EXPORTER_OTLP_TIMEOUT`, `OTEL_EXPORTER_OTLP_COMPRESSION` (only `gzip`/unset accepted), `OTEL_EXPORTER_OTLP_HEADERS`, TLS-related and `OTEL_SERVICE_NAME`/`OTEL_RESOURCE_ATTRIBUTES` vars, validating shape before provider start;
- rejects `OTEL_TRACES_SAMPLER` values other than unset/`parentbased_always_on` with a typed `OtlpConfigError::ConflictingSampler`.

Fold the existing `endpoints()`/`ensure_grpc_protocol()`/`ENDPOINT_VARS`/`PROTOCOL_VARS` logic (`observability.rs:583-728`) into this module rather than duplicating it; keep `container_otlp()` rewrite behavior intact.

**Verify**: `cargo nextest run -p jackin-diagnostics --locked -E 'test(config)'` → new tests pass (write them per Test plan).

### Step 3: Resource + service identity

Add `pub struct ServiceIdentity { pub service_name: &'static str, pub app_mode: AppMode }` (`AppMode` comes from `jackin_telemetry::schema`). `build_resource(identity)` produces the full Resource from item 7 above (use `opentelemetry-semantic-conventions` / `jackin_telemetry::schema::std_attrs` constants; `service.instance.id` = `uuid::Uuid::new_v4()`; `process.pid` = `std::process::id()`; `process.executable.name` from `std::env::current_exe()` file name; `container.id` in capsule read from `/proc/self/cgroup`/`/etc/hostname` best-effort — omit when unavailable). Thread `ServiceIdentity` through `init_tracing`/`init_capsule_tracing` signatures; update the four callers: host CLI (`jackin` one-shot = `app.mode=one_shot`, console = `interactive` — the host knows which at `crates/jackin/src/app.rs` where `RunDiagnostics::start` is called, line ~127), host daemon (`jackin-daemon`, `daemon`), capsule (`jackin-capsule`, `capsule`). `jackin-role` applies if/where the `jackin-role` binary initializes telemetry — search `grep -rn "init_tracing" crates/ --include='*.rs'` and cover every caller found.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features --locked` → pass, including a new test asserting the Resource carries exactly the contract attrs and NO `parallax.run.id`/session/invocation keys.

### Step 4: Sampler, batching bounds, gzip, retry

In `build_otlp_providers` (`observability.rs:751`):
- tracer provider: `.with_sampler(Sampler::ParentBased(Box::new(Sampler::AlwaysOn)))`;
- span batch: queue 2048, batch ≤ 512, 1 s delay, 5 s export timeout; log batch: queue 4096, same delay/timeout (use the 0.32 SDK batch-config builder — `BatchConfigBuilder` — inside the existing async-runtime processors);
- metric reader interval 5 s → **30 s** (`observability.rs:1236`);
- exporters: enable gzip compression (`.with_compression(Compression::Gzip)`) and route construction through new `observability/retry.rs`, the only module referencing the experimental retry API;
- keep span limits (64/32) as-is for now — plan 004 owns the full limits table.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features --locked` → pass. `grep -rn "experimental" crates/jackin-diagnostics/src/ --include='*.rs' | grep -v retry.rs | grep -i grpc` → no gRPC-retry references outside `retry.rs`.

### Step 5: Ordered shutdown + typed health

- New `observability/health.rs`: `pub struct TelemetryHealth` with atomics for export attempts/successes/failures per signal (wrap the exporters with a thin counting delegate, or hook the outer flush/export result where the code already observes it — `flush_and_shutdown` at `:499`), facade rejections (plumbed by plan 004 — expose the counter now), flush result, shutdown result, active signals. `pub fn telemetry_health_snapshot() -> TelemetryHealth`.
- Make `OtlpProviders::flush_and_shutdown` enforce the order tracer → logger → meter, run under a 5 s coordinated budget, and only then allow the dedicated runtime to stop. Confirm every exit path (host `ActiveRunGuard::drop` in `run.rs:109-129`, capsule `FlushGuard::drop` in `jackin-usage/src/telemetry.rs:76-80`) reaches it exactly once.
- Guarantee the no-op fast path: when `resolve_otlp_config` returns `Ok(None)`, assert no `OTEL_RUNTIME` is created (`otel_runtime()` must not be called; make provider construction the only caller).

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features --locked` → pass, including new tests: shutdown order recorded; disabled path creates no runtime (assert via a test-only counter or `OnceLock` state accessor).

### Step 6: Ripple + conformance

Run the full suite; fix compile fallout in dependents mechanically (feature-name removals only — semantic call-site changes are out of scope). Regenerate the export-volume ratchet if the conformance test's default-mode counts changed (they should NOT in this plan — if they do, STOP): `cargo nextest run -p jackin-diagnostics --all-features -E 'test(conformance_export_volume)'` then `cargo xtask lint ratchet --print export-volume`.

**Verify**: `cargo nextest run --workspace --all-features --locked` → all pass; `cargo xtask ci --only lint` → exit 0.

## Test plan

New tests in the respective sibling `tests.rs` files (layout rule: single `tests.rs` per module), modeled on the existing style of `crates/jackin-diagnostics/src/observability/otlp/tests.rs`:
- `config`: endpoint precedence (base vs per-signal), `OTEL_SDK_DISABLED=true` → `None`, bad protocol → typed error, `OTEL_TRACES_SAMPLER=always_off` → `ConflictingSampler`, header/timeout parsing.
- `resource`: full attribute set per `ServiceIdentity`; `service.instance.id` differs across two builds; forbidden keys absent.
- `health`: counters move on export success/failure (use the in-memory `TestExport` at `observability.rs:1032-1037` / `observability_test_support.rs`).
- `shutdown`: order tracer→logger→meter; double-shutdown is a no-op.
- Disabled fast path: no runtime, no providers, `init_tracing` returns `Ok(false)`.

## Done criteria

- [ ] `grep -rn 'feature = "otlp"' crates/ --include='*.rs' --include='*.toml'` returns no matches
- [ ] `grep -rn "OTEL_SDK_DISABLED\|ParentBased" crates/jackin-diagnostics/src/` shows both implemented
- [ ] `cargo nextest run --workspace --all-features --locked` exits 0
- [ ] `cargo hack check -p jackin-diagnostics --feature-powerset --all-targets --locked` exits 0
- [ ] `cargo xtask ci --only lint` exits 0
- [ ] gRPC-retry API referenced only in `observability/retry.rs`
- [ ] `plans/unified-otel-observability/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:
- `opentelemetry-otlp` 0.32 lacks a named feature this plan assumes (`gzip-tonic`, `experimental-grpc-retry`, a TLS feature compatible with tonic 0.14.6) — report the actual feature list from the registry manifest instead of substituting.
- Removing the `otlp` feature breaks a dependent in a way requiring semantic (not mechanical) changes outside the in-scope file list.
- The conformance export-volume counts change (this plan must not alter emitted signals).
- The 0.32 SDK's async-runtime batch processors reject the required `BatchConfig` (queue/batch/delay/timeout) knobs.

## Maintenance notes

- `observability/retry.rs` is deliberately the only experimental-API surface; when opentelemetry-rust stabilizes retry, swap it there and delete the pin comment.
- Plans 003 (bridges), 004 (facade/limits), 012 (validate command + daemon health RPC) consume `TelemetryHealth` — keep its fields additive.
- Reviewer focus: no telemetry work may ever run on the app's current-thread runtime (the manifest comment at `Cargo.toml:68-77` explains the deadlock); all provider construction must stay inside the dedicated runtime's `enter()`.
