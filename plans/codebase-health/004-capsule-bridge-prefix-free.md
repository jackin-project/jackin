# Plan 004: Prefix-free, schema-complete capsule OTLP export (split render from export)

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-usage/src/logging.rs crates/jackin-usage/src/telemetry.rs crates/jackin-diagnostics/src/observability.rs`
> Mismatch with "Current state" = STOP. Requires plan 001 (registry) landed.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/codebase-health/001-telemetry-event-registry.md
- **Category**: tech-debt (telemetry contract)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

The wire contract is explicit: "OTLP must never receive `[jackin …]` or `[jackin-capsule …]` bodies." Today every capsule `clog!`/`cdebug!` call formats a bracket-prefixed line and forwards it verbatim to OTLP via `bridge_log`, with **no** canonical attributes (`event.name`, `event.outcome`, `jackin.component`, `jackin.category`, `error.type`) and no redaction pass at that boundary. That is the entire capsule surface (~250 call sites) violating the Body row, the attribute schema, and matrix point 3 ("neither sink exposes preformatted `[jackin-capsule …]` text"). The roadmap also requires preserving the operator-visible capsule file behavior through the facade before removing `clog!`/`cdebug!` rendering — so this plan splits rendering (keeps prefixes) from export (loses them), rather than deleting the macros.

## Current state

- `crates/jackin-usage/src/logging.rs:221-227` — `clog!`:

```rust
macro_rules! clog {
    ($($arg:tt)*) => {{
        let line = format!("[jackin-capsule] {}", format_args!($($arg)*));
        $crate::telemetry::bridge_log($crate::telemetry::BridgeLevel::Info, &line);
        ...
```

  `cdebug!` (`:236-244`) uses `[jackin-capsule debug]`; `cdebug_local!` (`:264`) skips the bridge. Similar `cwarn!`/`cerror!` variants exist below (read the whole macro block before editing).
- `crates/jackin-usage/src/telemetry.rs:104-115` — `bridge_log` emits `tracing::info!(target: "jackin_capsule", "{message}")` verbatim per level; `otlp_active()` gates it.
- `crates/jackin-diagnostics/src/observability.rs:921` region — `init_capsule` bridges target `jackin_capsule` through `OpenTelemetryTracingBridge` with no taxonomy layer; `component_for` (`observability.rs:1486-1492`) sniffs the `[jackin-capsule` prefix to classify component — that sniff dies with this plan.
- `crates/jackin-usage/src/logging.rs:95-102` — capsule file logger also reads `JACKIN_DEBUG` directly (plan 006 centralizes; avoid conflicting edits — coordinate via rebase if 006 lands first).
- Test locking the wrong shape: `observability/otlp/tests.rs:431-444` asserts a raw `jackin_capsule`-target body exports as-is.
- Capsule file behavior to preserve: file/stderr lines keep their `[jackin-capsule]`/`[jackin-capsule debug]` prefixes byte-for-byte (operators grep these); the context banner at `crates/jackin-usage/src/logging.rs:167` region stays.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Usage crate | `cargo nextest run -p jackin-usage --all-features` | pass |
| Diagnostics | `cargo nextest run -p jackin-diagnostics --all-features` | pass |
| Capsule consumers | `cargo nextest run -p jackin-capsule` | pass |
| Lint | `cargo clippy -p jackin-usage -p jackin-diagnostics --all-targets --all-features -- -D warnings` | exit 0 |
| Cross-crate | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: `crates/jackin-usage/src/logging.rs` + `logging/tests.rs`, `crates/jackin-usage/src/telemetry.rs` + `telemetry/tests.rs`, `crates/jackin-diagnostics/src/observability.rs` (capsule init + `component_for` sniff removal) + its tests, `crates/jackin-usage/README.md` if module responsibilities shift.

**Out of scope**: migrating individual capsule call sites to `operation_log`/`operation_error` (plan 008 does the failure-path migration; this plan fixes the macro plumbing so ALL existing sites export clean bodies); JSONL schema (005); `JACKIN_DEBUG` resolution (006).

## Git workflow

Branch `refactor/capsule-bridge-prefix-free`; Conventional Commits; `git commit -s`; push after each commit.

## Steps

### Step 1: Restructure the macros — build the raw body once

In each capsule macro (`clog!`, `cdebug!`, `cwarn!`, `cerror!`): format the raw message once (no prefix), pass the raw message to the bridge, and add the prefix only in the file/stderr render path. Preserve exact rendered output (assert byte-identical rendering in tests).

**Verify**: `cargo nextest run -p jackin-usage` → rendering tests (existing + new) prove file/stderr lines unchanged.

### Step 2: Schema-stamp the bridge

Change `bridge_log` to accept structured context: `bridge_log(level, category: &'static str, event_name: &'static str, message: &str)` (category/event-name consts come from the plan-001 registry — add capsule event defs there if missing: e.g. `capsule.log`, `capsule.debug`, `capsule.warn`, `capsule.error` as the generic breadcrumb events, each with `jackin.component = "capsule"`). Emit tracing fields `event.name`, `jackin.category`, `jackin.component = "capsule"`, `event.outcome` (default `success`; `failure` for the error macro), and route the message through the redaction helper before emission (`jackin-usage` already depends on the diagnostics redact path via the facade — if not importable due to crate tiers, apply `jackin_usage`-local scrubbing consistent with `secret_scrub`; check `crates/jackin-usage/Cargo.toml` deps and the arch tier table in `crates/jackin-xtask/src/arch.rs` before adding a dependency; STOP if a new cross-tier dependency would be required).

Also stamp `session.id` (the capsule knows it — see how the context banner gets it, `logging.rs:167` region) and `parallax.run.id` when available, completing plan 002's capsule half.

**Verify**: `cargo nextest run -p jackin-usage -p jackin-diagnostics --all-features` → new exporter-backed test (step 4) pending; compile green.

### Step 3: Remove the prefix sniff

Delete the `message.starts_with("[jackin-capsule")` branch in `component_for` (or the whole fn if plan 001's registry already made it dead) — component now arrives as an explicit field.

**Verify**: `grep -n "jackin-capsule" crates/jackin-diagnostics/src/observability.rs` → no prefix-sniffing matches.

### Step 4: Flip the exporter test + add negative assertions

Rewrite `observability/otlp/tests.rs:431-444`: a bridged capsule log must export a prefix-free body AND carry `event.name`/`jackin.component=capsule`/`jackin.category`. Add a negative sweep: no captured record body starts with `[`.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → pass. `cargo nextest run -p jackin-capsule` → pass (call sites unchanged, macro-internal change). `cargo xtask ci --fast` → exit 0.

## Test plan

- `jackin-usage/logging/tests.rs`: byte-identical file/stderr rendering before/after (capture via the existing test sink; `drain_debug_buffer_for_test` pattern in diagnostics may have a usage-side equivalent — find the current logging tests first and extend them).
- `jackin-usage/telemetry/tests.rs`: bridge receives raw body + structured fields.
- `jackin-diagnostics/observability/otlp/tests.rs`: prefix-free capsule export with canonical attrs; global no-`[`-body sweep.

## Done criteria

- [x] `grep -rn 'format!("\[jackin-capsule' crates/jackin-usage/src/logging.rs` shows prefixes only in render paths, and the value passed to `bridge_log` is prefix-free (read the macro to confirm)
- [x] Exporter test proves capsule records: prefix-free body, `event.name`, `jackin.component=capsule`, `jackin.category`, `session.id`
- [x] Prefix sniff removed from `component_for`
- [x] Capsule file/stderr rendering byte-identical (tests prove)
- [x] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- Drift vs excerpts.
- Redaction requires a dependency that violates the arch tier gate (`cargo xtask lint arch --strict` fails) — report the tier conflict.
- Preserving byte-identical rendering conflicts with the macro restructure at some call-site pattern (e.g. format-args capturing) — enumerate the pattern, stop.
- More than a handful of capsule call sites turn out to pass pre-formatted `[jackin…` strings INTO the macros (double-prefix hazard) — enumerate, stop.

## Maintenance notes

- Plan 008 migrates failure-prone call sites onto `operation_error`; the generic breadcrumb events added here are the floor, not the target shape.
- Plan 009 asserts prefix-freedom continuously from the real host-to-capsule path.
- Reviewers: any new capsule macro variant must route through `bridge_log`'s structured signature.

## Execution notes

- component_for prefix sniff already gone after plan 001 registry routing.
