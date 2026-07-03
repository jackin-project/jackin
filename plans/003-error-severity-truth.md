# Plan 003: Make failures export at failure severity — capsule WARN/ERROR tiers, host error path, panic capture

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-usage/src/logging.rs crates/jackin-usage/src/telemetry.rs crates/jackin-capsule/src/attach_protocol.rs crates/jackin-diagnostics/src/run.rs crates/jackin-core/src/launch_progress.rs`
> On any excerpt mismatch below, STOP.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/001-otlp-export-test-seam.md
- **Category**: bug
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

Measured against a live backend: **968,593 DEBUG + 8,977 INFO logs vs 4 WARN and 1 ERROR**, while 1,686 log bodies contained "error/failed/panic". The architecture makes this inevitable:

1. The capsule's only bridge maps macro tier → severity (`clog!`→INFO, `cdebug!`→DEBUG); **no capsule failure can ever be WARN or ERROR** — attach failures, poisoned mutexes, spawn failures all export as INFO.
2. The host's one ERROR emit path (`RunDiagnostics::error`) has **zero production callers** and is not even on the `LaunchDiagnostics` port trait, so launch crates cannot reach it; `stage()` exports `stage_failed` at INFO.
3. Panics: the capsule panic hook writes only to `multiplexer.log`/stderr (never bridged to OTLP); the host installs **no panic hook at all**.
4. A stable error taxonomy exists (`ErrorCode` E001–E016 in `crates/jackin/src/error.rs`) but never touches telemetry; error "classification" for display is substring matching on prose.

A backend alerting on severity ≥ WARN for jackin sees nothing. This plan makes severity carry truth and stamps `error.type` from the existing taxonomy.

## Current state

Capsule macros — `crates/jackin-usage/src/logging.rs:141-164`:

```rust
macro_rules! clog {
    ($($arg:tt)*) => {{
        let line = format!("[jackin-capsule] {}", format_args!($($arg)*));
        $crate::logging::write_line(&line);
        $crate::telemetry::bridge_log(false, &line);
    }};
}
macro_rules! cdebug { /* same, prefix "[jackin-capsule debug]", bridge_log(true, …) */ }
```

Bridge — `crates/jackin-usage/src/telemetry.rs:60-69`:

```rust
pub fn bridge_log(debug: bool, message: &str) {
    if !otlp_active() { return; }
    if debug { tracing::debug!(target: "jackin_capsule", "{message}"); }
    else     { tracing::info!(target: "jackin_capsule", "{message}"); }
}
```

Capsule panic hook — `crates/jackin-usage/src/logging.rs:100-110`: calls `write_line("[jackin-capsule] PANIC: …")` + backtrace; never `bridge_log`. Capsule OTLP flush: `telemetry::shutdown()` / `FlushGuard` (`telemetry.rs:41-49,73-77`).

Attach client read loop — `crates/jackin-capsule/src/attach_protocol.rs:292-311` (all four failure arms are `clog!` → INFO):

```rust
result = stream.read_exact(&mut tag) => {
    if let Err(e) = result {
        crate::clog!("attach client: socket read failed: {e}");
        break;
    }
    ...
    Ok(None) => { crate::clog!("attach client: EOF mid-frame (tag={:#04x})", tag[0]); break; }
    Err(e)   => { crate::clog!("attach client: frame decode failed (tag={:#04x}): {e}", tag[0]); break; }
```

Comment at `crates/jackin-runtime/src/runtime/attach.rs:778` confirms the *normal* detach path surfaces as `early eof` on this read — expected shutdown and real failure share one INFO line.

Host error path — `crates/jackin-diagnostics/src/run.rs:285-287`:

```rust
pub fn error(&self, kind: &str, message: &str) {
    crate::observability::emit_jsonl_error(&self.run_id, kind, message, None, None);
}
```

`emit_jsonl_error` → `tracing::error!` (`observability.rs:1140-1179`). Sole caller: `crates/jackin-diagnostics/src/tests.rs:119`. Port trait `LaunchDiagnostics` (`crates/jackin-core/src/launch_progress.rs:~228`) exposes `compact`/`stage`/`path`/`command_output_path`/`run_id` — no `error`.

`stage()` — `run.rs:289-351`: all kinds (including `stage_failed`, emitted by `crates/jackin-launch-tui/src/progress.rs:158-163`) route to `emit_jsonl_event` → INFO.

Error taxonomy — `crates/jackin/src/error.rs`: `ErrorCode::E001..E016` with `as_str()` (`error.rs:35-50`), `JackinError` enum; consumed only by stderr rendering (`crates/jackin/src/main.rs:155-156`). Substring classifier to replace eventually: `crates/jackin-runtime/src/runtime/launch/failure.rs:13-29` (`text.contains("docker")` etc.) — display-only, DO NOT touch in this plan.

Conventions: two-tier telemetry is an ENGINEERING.md hard rule — this plan does not remove tiers, it adds severity *within* the always-on tier. Comments: non-obvious WHY only. Tests in `<module>/tests.rs`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Format / lint / check | `cargo fmt --check` ; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` ; `cargo check --all-targets --all-features` | exit 0 each |
| Tests | `cargo nextest run --all-features` | all pass |
| Capsule crate tests | `cargo nextest run -p jackin-capsule -p jackin-usage --all-features` | all pass |

## Scope

**In scope**:

- `crates/jackin-usage/src/logging.rs` — macro severity variants + panic-hook bridge
- `crates/jackin-usage/src/telemetry.rs` — `bridge_log` severity parameter
- `crates/jackin-capsule/src/attach_protocol.rs` — EOF classification (this file only; other capsule reclassifications are listed as a bounded follow-up in Maintenance)
- `crates/jackin-capsule/src/lib.rs` — re-export new macros
- `crates/jackin-diagnostics/src/run.rs` — `stage()` failure severity; host panic-hook helper
- `crates/jackin-diagnostics/src/observability.rs` — `emit_jsonl_event_with_level` severity plumb (extend the existing enum) + `error.type` field
- `crates/jackin-core/src/launch_progress.rs` — add `error` to the `LaunchDiagnostics` trait (with default impl delegating to `compact` is NOT acceptable — implement properly on `RunDiagnostics`)
- `crates/jackin/src/app.rs` — install host panic hook
- `crates/jackin-launch-tui/src/progress.rs` — `stage_failed` passes `error.type` when available
- `crates/jackin-diagnostics/src/tests.rs`, `crates/jackin-diagnostics/src/observability/otlp/tests.rs`, new `crates/jackin-usage/src/telemetry/tests.rs` — tests
- `docs/content/docs/reference/runtime/diagnostics.mdx` — severity model paragraph

**Out of scope**:

- Reclassifying every one of the 176 capsule `clog!` sites (follow-up batch; this plan converts the attach-protocol arms and adds the mechanism).
- Span status / `otel.status_code` (plan 007 — spans must wrap work first to be worth stamping).
- Body/attribute restructuring, `event.name` (plan 006). Fingerprints change there.
- Redaction (plan 005). The substring classifier in `failure.rs` (display-only).

## Git workflow

- Propose branch `fix/telemetry-error-severity`; wait for operator confirm. `git commit -s` per logical unit (`fix(capsule): …`, `fix(diagnostics): …`), push after every commit.

## Steps

### Step 1: Severity through the capsule bridge

In `crates/jackin-usage/src/telemetry.rs`, replace `bridge_log(debug: bool, …)` with:

```rust
#[derive(Debug, Clone, Copy)]
pub enum BridgeLevel { Debug, Info, Warn, Error }

pub fn bridge_log(level: BridgeLevel, message: &str) {
    if !otlp_active() { return; }
    match level {
        BridgeLevel::Debug => tracing::debug!(target: "jackin_capsule", "{message}"),
        BridgeLevel::Info  => tracing::info!(target: "jackin_capsule", "{message}"),
        BridgeLevel::Warn  => tracing::warn!(target: "jackin_capsule", "{message}"),
        BridgeLevel::Error => tracing::error!(target: "jackin_capsule", "{message}"),
    }
}
```

In `logging.rs`, keep `clog!`/`cdebug!` exactly as-is except the bridge call becomes `bridge_log(BridgeLevel::Info, …)` / `(BridgeLevel::Debug, …)`, and add two sibling macros with the SAME file/stderr behavior as `clog!` (always-on tier, `[jackin-capsule]` prefix on the file line) but Warn/Error bridge levels:

```rust
#[macro_export]
macro_rules! cwarn { /* clog body, bridge_log(BridgeLevel::Warn, &line) */ }
#[macro_export]
macro_rules! cerror { /* clog body, bridge_log(BridgeLevel::Error, &line) */ }
```

Re-export from `crates/jackin-capsule/src/lib.rs` beside the existing `pub use jackin_usage::{cdebug, clog};` (line 55).

**Verify**: `cargo check --all-targets --all-features` → exit 0.

### Step 2: Classify the attach read-loop arms

In `attach_protocol.rs:292-311`:

- `read_exact` error where `e.kind() == std::io::ErrorKind::UnexpectedEof` → `cdebug!("attach client: socket closed (client detached)")` — this is the *expected* takeover/shutdown path (see `attach.rs:778` comment); a routine detach must not read like a failure.
- Any other `read_exact` error → `cerror!("attach client: socket read failed: {e}")`.
- `Ok(None)` EOF mid-frame → `cwarn!` (protocol truncation: abnormal but recovered by reconnect).
- Frame decode error → `cerror!`.
- `cmd_tx` closed → leave `clog!` (daemon shutting down — lifecycle, not failure).
- Socket write failure (`:320`) → `cwarn!` if `UnexpectedEof`-like/`BrokenPipe` (client vanished — routine), else `cerror!`.

**Verify**: `cargo nextest run -p jackin-capsule --all-features` → pass; `rg -n "socket read failed" crates/jackin-capsule/src/attach_protocol.rs` shows it inside `cerror!`.

### Step 3: Bridge panics to OTLP

Capsule (`logging.rs:100-110` hook): after the two `write_line` calls, add a best-effort bridge + flush:

```rust
jackin: crate::telemetry::bridge_log(BridgeLevel::Error, &format!("PANIC: {info}"));
crate::telemetry::shutdown(); // force_flush before the process dies
```

Constraint (name it in a comment): runs mid-unwind — must not panic itself; `shutdown()` is idempotent (`OTLP_ACTIVE` gate) and the `FlushGuard` double-shutdown is a no-op; do NOT bridge the multi-KB backtrace (file-only) — the ERROR record carries the one-line panic info; the backtrace stays in `multiplexer.log`.

Host: in `crates/jackin/src/app.rs` (near the existing `set_debug_mode` call at `app.rs:91`), install a hook that (a) records `run.error("panic", &info.to_string())` on the active run if any, (b) calls the existing teardown notice path, then (c) delegates to the previous hook. Put the helper in `jackin-diagnostics` (e.g. `pub fn install_host_panic_hook()` in `run.rs` or a small new `panic.rs` module file — remember: self-named module files, no `mod.rs`) so the app crate stays thin. OTLP flush on the host already happens via `ActiveRunGuard::drop` → but a panic may unwind past it only if `panic = unwind` and the guard is on the stack — to be safe, call `crate::observability::shutdown_otlp()` in the hook too; it is idempotent (`PROVIDERS.get()` + flush).

**Verify**: `cargo nextest run --all-features` → pass. Add the unit test from the Test plan.

### Step 4: Host error tier reachable from launch code

1. `run.rs`: change `stage()` so kinds `stage_failed` (and any `*_failed` suffix) route through `emit_jsonl_error` instead of `emit_jsonl_event` (keep all existing stage-span/duration logic identical). Simplest: thread a level into the final emit based on `kind.ends_with("_failed")`.
2. `observability.rs`: extend `emit_jsonl_error` / `emit_jsonl_event_with_level` with an optional `error_type: Option<&str>` field emitted as field name `error.type` — wait: tracing field names cannot contain dots in the `field = value` position unless quoted; use the quoted-name syntax `"error.type" = value`, which tracing supports via `error.type` field literal? Tracing DOES support dotted field names via `event!` with `otel`-style names only through the `valuable`/explicit syntax `tracing::error!(target: ..., { "error.type" } = ...)`? — **it supports them as raw identifiers with dots via the `field::` quoting**: use `error_type` as the tracing field name here, and rely on plan 006 (attribute taxonomy) to map/rename exported keys. Keep this plan simple: field name `error_type`.
3. `launch_progress.rs` trait: add `fn error(&self, kind: &str, message: &str, error_type: Option<&str>);` implement on `RunDiagnostics` (delegating to a new `error_typed`) and on any other trait impl (grep `impl LaunchDiagnostics` — expect the one in `run.rs:1116` plus possible test fakes; update them all).
4. `progress.rs::stage_failed` (`crates/jackin-launch-tui/src/progress.rs:145-163`): after the existing `stage("stage_failed", …)` call, also call `self.diagnostics.error("launch_failed", &summary, failure.error_code /* if the LaunchFailure struct carries one; if not, pass None */)`. Inspect `LaunchFailure` (same file, top) — if it has no error-code field, pass `None` and note in the commit body that E-code threading arrives when `JackinError` reaches this layer (Maintenance note).
5. Wire the ONE top-level host failure: find where a launch error is finally rendered for the operator (`crates/jackin/src/main.rs:155-156` downcasts `JackinError` and `.render()`s). Immediately before rendering, if a run is active: `run.error(code.as_str(), &err.to_string())` for `JackinError` (use `ErrorCode::as_str()`, e.g. `"E014"`) or `run.error("error", …)` otherwise. This single call site makes every fatal CLI failure an ERROR record with a stable code.

**Verify**: `cargo nextest run --all-features` → pass; new tests below green.

### Step 5: Tests + docs

Docs: `reference/runtime/diagnostics.mdx` — add a severity-model paragraph: DEBUG=firehose, INFO=lifecycle, WARN=handled/degraded, ERROR=operation failed; note `*_failed` stage kinds and fatal CLI errors export as ERROR with `error_type`.

**Verify**: full gate — `cargo fmt --check`; clippy; `cargo nextest run --all-features` → exit 0.

## Test plan

- `crates/jackin-usage/src/telemetry/tests.rs` (new file; declare `#[cfg(test)] mod tests;` in `telemetry.rs`): with a scoped `tracing` test subscriber capturing level+target (small hand-rolled `Layer` recording events — see `jackin-diagnostics` tests for subscriber patterns), assert `bridge_log(Warn|Error|Info|Debug, …)` emit at those levels on `target="jackin_capsule"`, and that all are suppressed when `otlp_active()` is false. `OTLP_ACTIVE` is a process-global `AtomicBool` — safe under nextest process-per-test; set it via a `#[cfg(test)] pub(crate) fn set_otlp_active_for_test(bool)`.
- `jackin-diagnostics/observability/otlp/tests.rs` (uses plan 001 seam): `stage_failed_exports_as_error` — drive `RunDiagnostics::stage("stage_failed", "derived image", "boom", None)` → exported log severity == Error. `fatal_error_carries_error_type` — `run.error_typed("E014", "capsule download failed", Some("E014"))` → ERROR record with `error_type` attribute.
- `attach_protocol` classification: pure-logic extraction test — factor the io-error→level decision into `fn classify_read_error(kind: std::io::ErrorKind) -> …` in `attach_protocol.rs` and unit-test it in `crates/jackin-capsule/src/attach_protocol/tests.rs` (file exists? if not, create + `#[cfg(test)] mod tests;`).
- Panic hooks: capsule — unit test `bridge_log` called from a panic-hook context doesn't panic (call the hook closure body with a fabricated `PanicHookInfo` is not constructible; instead test the extracted helper `fn panic_bridge_line(info: &str) -> String` and that `shutdown()` is idempotent by calling twice). Host — test `install_host_panic_hook()` is idempotent (double-install keeps one previous hook; use a `OnceLock` mirroring `PANIC_HOOK_INSTALLED` in `logging.rs:29`).

## Done criteria

- [ ] `rg -n "bridge_log\(true|bridge_log\(false" crates/` → no matches (old signature gone)
- [ ] `rg -n "clog!\(\"attach client: socket read failed" crates/jackin-capsule` → no matches (now `cerror!`, with EOF split out)
- [ ] `cargo nextest run --all-features` exits 0 incl. new tests; plan 001's `exported_error_log_is_error_severity` still green
- [ ] `rg -n "fn error" crates/jackin-core/src/launch_progress.rs` → trait method present
- [ ] Host + capsule panic hooks installed (grep `set_hook` → 2 sites: `logging.rs`, new diagnostics helper)
- [ ] clippy/fmt gates exit 0; `plans/README.md` row updated

## STOP conditions

- `LaunchFailure` restructuring is needed to thread an error code (only `None` plumbing is in scope — if you find yourself editing `LaunchFailure`'s shape, stop and report).
- The tracing macros reject the severity plumb in `emit_jsonl_event_with_level` without duplicating the whole macro arm per level — if the emit function grows a 4-way copy of the field list, extract a `macro_rules!` local to the function file; if that still fails clippy's cognitive-complexity, STOP and propose.
- Any existing test asserting INFO for `stage_failed` fails in a crate you did not expect (search first: `rg -rn "stage_failed" crates/*/src/**/tests.rs`).
- Capsule panic-hook flush deadlocks in a test (flush from within the OTel runtime) — report; do not ship a hook that can hang shutdown.

## Maintenance notes

- Follow-up batch (deliberately deferred): reclassify remaining capsule failure `clog!` sites to `cwarn!`/`cerror!` — candidates: `session.rs:472,525,1372`, `pid1.rs:60,86,116`, `daemon/compositor.rs:60,482`, `daemon.rs:873,1071`, `socket.rs:273`, `firewall.rs:137,150` (the baked-in `WARNING:` text), `exec.rs:170`. Mechanical after this plan; each is a one-line macro swap. Also: `tracing::warn!` sites in `jackin-runtime` double-emit with `run.compact` (`git_pull.rs:95+132`) — consolidate when touched.
- Plan 006 renames `error_type` → semconv `error.type` on the wire and adds `event.name`; it owns fingerprint stability.
- Reviewer scrutiny: severity inflation. WARN/ERROR must mean "operator should care" — push back on any reclassification of routine paths (expected EOF stays out of WARN).
