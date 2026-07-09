# Plan 041: Typed operation facade — one structured telemetry API, collapsed `debug_log!`, first adoption at ShellRunner

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat fabe88406..HEAD -- crates/jackin-diagnostics/src crates/jackin-core/src/debug_log.rs crates/jackin-docker/src/shell_runner.rs crates/jackin-usage/src/logging.rs crates/jackin-usage/src/telemetry.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition. Plan 018 landing IS expected drift —
> read its diff and continue; anything else is a real STOP.

## Status

- **Priority**: P1
- **Effort**: M-L
- **Risk**: MED (touches the two hottest logging paths; bounded by adopting at one choke point only)
- **Depends on**: plans/code-health/018-telemetry-drift-proofing.md (Step 2's semconv registry is where this plan's event names live; Step 1's single builder is the substrate)
- **Category**: tech-debt
- **Planned at**: commit `fabe88406`, 2026-07-09

## Why this matters

Every debug line jackin❯ exports today is console-formatted *before* export: `format_debug_line` bakes `[jackin debug {category}]` into the body, and the capsule macros bake `[jackin-capsule]`, so an OpenTelemetry backend can only filter by text search, never by dimension. The live-backend audit (the Parallax dossier) measured the result: 968,593 DEBUG log rows whose category exists only as a bracket prefix inside `body`. The roadmap (Phase 8 item 1) prescribes the fix: one typed operation API — `operation_span` / `operation_log` / `operation_error` / `operation_metric` — with per-sink renderers, adopted at shared choke points first. This plan builds that API, collapses the two duplicate `debug_log!` macro definitions into one, and adopts the facade at the single highest-value choke point (`ShellRunner`, which runs every host subprocess). Call-site mass-migration is explicitly NOT this plan — later waves adopt one choke point each.

## Current state

All excerpts verified by direct read at `fabe88406`.

- `crates/jackin-diagnostics/src/logging.rs:263-265` — the enabling condition for prefix-in-body:
  ```rust
  pub fn format_debug_line(category: &str, message: &str) -> String {
      format!("[jackin debug {category}] {message}")
  }
  ```
  `emit_debug_line` (`logging.rs:177-206`) formats the line first, then hands the *formatted* line to `crate::run::active_debug(category, &line)` — so the JSONL/OTLP tier receives the prefixed body.
- **Two identical `debug_log!` macro definitions** (the duplicate plan 018 explicitly left out of scope):
  - `crates/jackin-core/src/debug_log.rs:65-71` — routes through the `DebugLogSink` port (`GLOBAL_SINK` OnceLock, `NoopSink` default, `set_global_sink`); used by lower crates (e.g. `jackin-config/src/migrations.rs`).
  - `crates/jackin-diagnostics/src/lib.rs:70-76` — same signature/doc, but calls `$crate::is_debug_mode()` / `$crate::emit_debug_line()` directly. **225 call sites in 42 files** invoke it by path (`jackin_diagnostics::debug_log!`), e.g. `crates/jackin-console/src/services/role_source.rs:52`.
  - The sink adapter `crates/jackin-diagnostics/src/debug_log.rs:8-21` (`DiagnosticsDebugLog` implements the port by forwarding to `is_debug_mode`/`emit_debug_line`) is installed at `crates/jackin/src/app.rs:131` (`jackin_diagnostics::debug_log::install_debug_log_sink();`). After install, both macros behave identically; before install, the core macro is a no-op.
- Capsule macro stack: `crates/jackin-usage/src/logging.rs:195-260` — `clog!` (:195), `cdebug!` (:210), `ctrace_payload!` (:224), `cdebug_local!` (:238), `cwarn!` (:245), `cerror!` (:253). Each formats `[jackin-capsule…] {msg}`, calls `write_line`, then `crate::telemetry::bridge_log(level, &line)`. `bridge_log` (`crates/jackin-usage/src/telemetry.rs:73-84`) flattens to `tracing::info!(target: "jackin_capsule", "{message}")` etc. — prefix rides into the OTLP body. **This plan does not migrate these call sites**; it builds the API they will migrate to in later waves.
- `crates/jackin-diagnostics/src/observability.rs:23-55` — `pub mod otel_keys`: attribute-name constants only (`RUN_ID = "parallax.run.id"`, `COMPONENT`, `SCREEN_NAME = "jackin.screen.name"`, …). Plan 018 Step 2 extends this registry with metric-name and event-name constants — this plan mints operation/event names from that registry and adds any new ones there, never as inline literals.
- Span-attribute exemplar to copy: `crates/jackin-diagnostics/src/screen.rs:126-138` — builds `tracing::info_span!` then stamps attributes via `tracing_opentelemetry::OpenTelemetrySpanExt::set_attribute`, links via `span.add_link(ctx)`.
- In-memory export test rig: `observability.rs:911-952` — `TestExport`/`test_layers(debug, run_id)` with `InMemorySpanExporter`/`InMemoryLogExporter`, exercised by `observability/otlp/tests.rs` (asserts resource attrs, `error.type`, span status). Extend this suite; do not build a second rig.
- Adoption target `crates/jackin-docker/src/shell_runner.rs`:
  - `ShellRunner` struct :18-21; trait methods `run` (:223) and `run_captured` (:325).
  - Existing telemetry: `log_command` (:65-78) emits the console line via `emit_debug_line("cmd", …)`; `record_subprocess_done` (:85-91) records duration/exit into the JSONL run via `active_subprocess_done`.
  - Redaction already exists and must be reused: `redact_env_args` (:94-119), `redact_arg`/`is_sensitive_arg_key` (:121-149), `jackin_diagnostics::redact::redact_text`.
- Conventions that bind this plan:
  - `crates/jackin-usage/AGENTS.md`: "Shared logging tier is rooted here (`clog!`/`cdebug!`, re-exported via `jackin-diagnostics`) … do not introduce a parallel logging path." The facade is NOT a parallel path — it is the structured tier the macros bridge into; the console/file renderers stay exactly where they are.
  - `crates/jackin-diagnostics/AGENTS.md`: "New logging uses these macros, not `log::`/`tracing::` directly." Step 6 revises this rule (the facade becomes the contract; the macros stay as console/file renderers) — the AGENTS edit is part of this plan, not a violation of it.
  - Module layout: self-named files, no `mod.rs`; ALL tests for a module in a single sibling `tests.rs` (see `crates/AGENTS.md`).
  - Comments: non-obvious WHY only. No `unwrap`/`expect`/`panic` in production code (workspace denies them).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Diagnostics tests | `cargo nextest run -p jackin-diagnostics` | all pass |
| Docker-crate tests | `cargo nextest run -p jackin-docker` | all pass |
| Capsule closure tests | `cargo nextest run -p jackin-usage -p jackin-capsule` | all pass |
| Whole-workspace compile | `cargo check --workspace --all-targets --locked` | exit 0 |
| Workspace clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-diagnostics/src/operation.rs` (create) + `crates/jackin-diagnostics/src/operation/tests.rs` (create)
- `crates/jackin-diagnostics/src/lib.rs` (delete the duplicate macro, re-export core's, `pub mod operation`, re-export the facade fns)
- `crates/jackin-diagnostics/src/observability.rs` — only if a new event-name const must be added to the (post-018) registry
- `crates/jackin-diagnostics/README.md` + `crates/jackin-diagnostics/AGENTS.md` (structure + contract updates)
- `crates/jackin-docker/src/shell_runner.rs` + `crates/jackin-docker/src/shell_runner/tests.rs`
- `docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx` — Phase 8 item 1 status note
- `ENGINEERING.md` — extend the two-tier telemetry bullet with the facade sentence

**Out of scope** (do NOT touch):
- The 225 `jackin_diagnostics::debug_log!` call sites and the 289 capsule-macro call sites — they keep compiling unchanged; migration is later waves.
- `crates/jackin-usage/src/logging.rs` / `telemetry.rs` — the capsule renderers and bridge are untouched this wave.
- Per-sink `EnvFilter`s and `JACKIN_DEBUG` retirement (plan 043), metric instruments beyond the `operation_metric` API shape (plan 042), the conformance lane (plan 044).
- `crates/jackin-core/src/debug_log.rs` — the port and macro stay as-is (they become the single definition).

## Git workflow

- Branch off `main`: `refactor/telemetry-operation-facade`.
- Conventional Commits, sign every commit (`-s`), push after every commit. PR to `main`; do not merge.
- This touches the capsule dependency closure (`jackin-diagnostics`), so the PR body needs the capsule smoke block from `.github/PULL_REQUEST_TEMPLATE.md` — copy it verbatim.

## Steps

### Step 1: Collapse the duplicate `debug_log!` into the port-based definition

In `crates/jackin-diagnostics/src/lib.rs`, delete the `macro_rules! debug_log` block (lines 61-76, including its doc comment) and add `pub use jackin_core::debug_log;` beside the other re-exports so the 225 existing `jackin_diagnostics::debug_log!` path invocations keep resolving. The surviving definition is `jackin-core`'s port-based one, whose behavior after `install_debug_log_sink()` is identical (the adapter at `crates/jackin-diagnostics/src/debug_log.rs:8-21` forwards to the same `is_debug_mode`/`emit_debug_line`).

Before deleting, confirm install ordering: `grep -rn "install_debug_log_sink" crates/jackin/src/` and read the call path from `main` — the install at `app.rs:131` must run before any code path that emits `jackin_diagnostics::debug_log!`. Check the binary's startup sequence (`crates/jackin/src/main.rs` → `app.rs`) for earlier `debug_log!` emissions; also check `crates/jackin-dev` and any other binary that links jackin-diagnostics for their own install calls. If a binary emits before installing, add the `install_debug_log_sink()` call at the top of that binary's main — report it in the PR body.

**Verify**: `cargo check --workspace --all-targets --locked` → exit 0; `grep -rn "macro_rules! debug_log" crates/` → exactly one hit (`jackin-core/src/debug_log.rs`); `cargo nextest run -p jackin-diagnostics` → all pass.

### Step 2: Create the typed operation API

Create `crates/jackin-diagnostics/src/operation.rs` (declare `pub mod operation;` in lib.rs, re-export the four fns from the root). The API (names final, signatures adjustable to what compiles cleanly):

- `pub fn operation_span(name: &'static str, attrs: &[(&'static str, String)]) -> tracing::Span` — builds `tracing::info_span!("operation", otel.name = name)` and stamps each attr via `OpenTelemetrySpanExt::set_attribute`, exactly like `screen.rs:126-138`. `name` must be a registry const (post-018 `otel_keys`/sibling module), never an inline literal.
- `pub fn operation_log(level: OperationLevel, event_name: &'static str, category: &'static str, body: &str, attrs: &[(&'static str, String)])` — emits ONE structured tracing event with explicit fields (`event.name`, `jackin.category`, plus the fixed attr schema) whose message is the *clean* body (no bracket prefix), AND mirrors a console/file line through the existing renderers: `emit_compact_line(category, body)` for `Info`+, `emit_debug_line(category, body)` for `Debug` (those helpers still apply the console prefix — that is their job; the prefix now exists only at the render boundary). `OperationLevel` = `Info | Debug | Warn | Error`.
- `pub fn operation_error(error_type: &'static str, body: &str, attrs: &[(&'static str, String)])` — ERROR-severity event carrying `error.type`, marks the current span's status Error (see how `emit_jsonl_error_typed` at `observability.rs:~1223` does it), mirrors to console via `emit_compact_line`.
- `pub fn operation_metric(name: &'static str, value: u64, attrs: &[(&'static str, String)])` — thin no-op-when-no-provider recorder. This wave: implement as a `u64` counter add on a lazily-created meter from the installed provider (or a no-op if none); plan 042 builds the real instrument set on top. Keep it 20 lines.

Design constraints (from the dossier's "Rust Implementation Guidance" and the repo's own patterns): explicit fields over formatted strings; free-text `body` passes through `crate::redact::redact_text` before emission; attrs are low-cardinality only — document in the module `//!` header that full command strings, full URLs, raw payloads, and container ids are forbidden as attrs (redacted/summarized values only).

Note on `tracing` static-field limits: `tracing::event!` requires field names known at compile time. The fixed schema (`event.name`, `jackin.category`, `event.outcome`, `error.type`, `message`) is enough for this wave; dynamic extra attrs go on the *span* (`operation_span` attrs), not the event. If a per-event dynamic attr turns out to be required, record it as a follow-up rather than reaching for `valuable`/hacks.

**Verify**: `cargo check -p jackin-diagnostics` → exit 0.

### Step 3: Prove the export shape with the in-memory rig

In `crates/jackin-diagnostics/src/operation/tests.rs`, using `observability`'s existing `test_layers` (`observability.rs:919-952` — it is `pub(super)`; if module visibility blocks reuse, widen to `pub(crate)` with a `reason`-less plain visibility change, not a suppression):

1. With the test subscriber installed and `operation_log(Debug, EVENT, "docker", "container inspected", &[…])` emitted: the captured log record's body is exactly `container inspected` — assert it does NOT contain `[jackin debug` — and its attributes include `event.name` and `jackin.category = "docker"`.
2. `operation_error(...)` yields severity ERROR and an `error.type` attribute (model the assertions on the existing `otlp/tests.rs` error-path tests).
3. `operation_span` + attrs: exported span carries `otel.name` and the stamped attributes.
4. Console mirror: use `begin_debug_buffering`/`drain_debug_buffer_for_test` (`logging.rs:159-175`) to assert the console line DOES carry the `[jackin debug docker]` prefix — prefix at render boundary, not in export.

**Verify**: `cargo nextest run -p jackin-diagnostics` → all pass including the 4+ new tests.

### Step 4: Adopt at ShellRunner

In `crates/jackin-docker/src/shell_runner.rs`, wrap the bodies of `run` (:223) and `run_captured` (:325) in an `operation_span` (name: a `process.execute`-style const from the registry) entered for the duration, with attrs: `process.command` = program only, `process.args_redacted` = `redact_env_args(args).join(" ")`, and on completion `process.exit_code`. On spawn/read failure, `operation_error("process_spawn_error", …)` (or the error-type string that matches the existing failure vocabulary — grep `error_typed` callers for precedent). Keep `log_command` (console line) and `record_subprocess_done` (JSONL) exactly as they are — they are the other sinks, not duplicates.

Add to `crates/jackin-docker/src/shell_runner/tests.rs` (follow the existing tests' structure): one test running a trivial command (`echo`/`true` — the suite already spawns processes; match its pattern) under the diagnostics test subscriber, asserting a span named `process.execute` with `process.exit_code = 0` was exported; one asserting args redaction in the span attr (`-e FOO=bar` → `FOO=<redacted>`).

**Verify**: `cargo nextest run -p jackin-docker` → all pass; `cargo clippy -p jackin-docker -p jackin-diagnostics --all-targets -- -D warnings` → exit 0.

### Step 5: Contract and docs updates

1. `crates/jackin-diagnostics/AGENTS.md`: replace the "New logging uses these macros, not `log::`/`tracing::` directly" rule with: new telemetry goes through the typed operation API (`operation_span`/`operation_log`/`operation_error`/`operation_metric`); `clog!`/`cdebug!`/`debug_log!` remain the console/file renderers and stay legal at existing sites; names come from the registry, never inline literals.
2. `crates/jackin-diagnostics/README.md`: add `operation.rs` to the structure table with its tests link; add the facade to the public-API section.
3. `ENGINEERING.md`: extend the two-tier telemetry bullet: the structured tier's API is the operation facade.
4. Roadmap Phase 8 item 1: mark the facade shipped-at-first-choke-point, macro-stack unification in progress (`debug_log!` collapsed; capsule macros pending), remaining adoption waves listed.

**Verify**: `cargo xtask lint agents && cargo xtask docs repo-links && cargo xtask roadmap audit` → all pass.

### Step 6: Full gate

**Verify**: `cargo xtask ci --fast` → `ci gate OK`; `cargo nextest run -p jackin-usage -p jackin-capsule` → all pass (proves the capsule closure is unbroken).

## Test plan

- New: 4+ rig tests in `operation/tests.rs` (clean body, category attr, error.type, console-prefix mirror), 2 ShellRunner span tests. Pattern: `observability/otlp/tests.rs` for export assertions; existing `shell_runner/tests.rs` for process tests.
- Regression: full `-p jackin-diagnostics -p jackin-docker -p jackin-usage -p jackin-capsule` suites; `cargo check --workspace` proves the macro collapse broke no path invocation.

## Done criteria

- [ ] `grep -rn "macro_rules! debug_log" crates/` → exactly 1 hit (jackin-core)
- [ ] `jackin_diagnostics::debug_log!` call sites compile unchanged (workspace check green)
- [ ] `operation_span`/`operation_log`/`operation_error`/`operation_metric` exist, documented, re-exported from the crate root
- [ ] Rig test proves: exported body prefix-free, console line prefixed, `event.name`/`jackin.category`/`error.type` attributes present
- [ ] ShellRunner `run`/`run_captured` emit `process.execute` spans with redacted args + exit code
- [ ] AGENTS/README/ENGINEERING/roadmap updated; `cargo xtask lint agents` green
- [ ] `cargo xtask ci --fast` → `ci gate OK`
- [ ] `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- Plan 018 has not landed (no metric/event-name registry beside `otel_keys`) — this plan mints names from it.
- The rig test in Step 3 shows the `OpenTelemetryTracingBridge` does NOT map the explicit tracing fields (`event.name = …`) to log-record attributes — the facade's export contract would be broken; report what the bridge actually emits instead of working around it.
- Step 1's startup audit finds a `debug_log!` emission before `install_debug_log_sink()` that cannot be fixed by moving the install call earlier in that binary's main.
- Any step seems to require editing `jackin-usage`'s macros or migrating call sites en masse.

## Maintenance notes

- Adoption waves (each its own plan, in the dossier's order — `crates/jackin-docker/src/net.rs` HTTP/download helpers next, then Docker lifecycle in `docker_client.rs`, then the launch stage/timing API in `run.rs`, then capsule attach/session): every wave converts ONE choke point and its tests, nothing else.
- Plan 042 replaces `operation_metric`'s minimal recorder with the real instrument set; plan 044's conformance lane asserts this plan's prefix-free contract permanently.
- Reviewer scrutiny: the Step 1 collapse must be behaviorally invisible (port adapter identical post-install); Step 4 must not double-report — the span is new signal, `record_subprocess_done` and `log_command` continue serving JSONL/console.
