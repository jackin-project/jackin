# Plan 008: Layered verbosity — `--telemetry-level` / `--telemetry-category`, splitting telemetry export from the `--debug` UI switch

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin/src/cli.rs crates/jackin/src/app.rs crates/jackin-diagnostics/src crates/jackin-usage/src crates/jackin-runtime/src/runtime/launch/launch_runtime.rs ENGINEERING.md TESTING.md`
> Plans 001–007 reshape the diagnostics internals this builds on — verify the
> `export_filter_directive` builder from plans 001/002 exists before starting;
> STOP if it does not.

## Status

- **Priority**: P2
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/002-export-filter-allowlist.md, plans/003-error-severity-truth.md, plans/005-redaction-boundary.md, plans/006-structured-event-taxonomy.md
- **Category**: dx / direction
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

One boolean (`--debug` / `JACKIN_DEBUG`) currently controls ~8 unrelated concerns: OTLP export level, JSONL firehose capture, the cockpit debug chip, extra dialog rows, the startup banner + Enter-gate, plain build output, capsule debug rows, and the container env passthrough. An operator cannot raise telemetry verbosity without also changing the UI, and cannot get the UI extras without widening export. Meanwhile the exported DEBUG tier is a single bucket: byte/frame/mouse internals (should be TRACE, opt-in) share a level with cache decisions and command summaries (useful DEBUG) — measured result: 968,593 DEBUG vs 8,977 INFO records in one live store. The research doc (`docs/content/docs/reference/research/agent-telemetry/parallax-observability-findings.mdx:186-226`) specifies the target contract; PRERELEASE.md §"no migration shims" (line 7) explicitly permits replacing the env/CLI surface without compatibility aliases. This plan ships `--telemetry-level {info|debug|trace}` + `--telemetry-category`, keeps `--debug` as the operator-UI switch only, and moves the firehose sites to TRACE.

## Current state

- Flag: `crates/jackin/src/cli.rs:84-91` (verified) — `--debug`, global, `env = "JACKIN_DEBUG"`.
- Boolean consumers (from the audited inventory; re-grep before editing):
  - Export level: `observability.rs` — `let level = if debug { "debug" } else { "info" }` (host `:792`; capsule `capsule_debug()` `:919-926`).
  - JSONL firehose gate: `RunDiagnostics::debug` early-return (`run.rs:476-482`); `debug_log!` macro gate (`jackin-diagnostics/src/lib.rs:62-68`); `is_debug_mode` atomics (`logging.rs:15-23`); capsule `debug_enabled` (`jackin-usage/src/logging.rs:37-39,60-66`).
  - UI-only consumers (KEEP on `--debug`): debug chip (`jackin-launch-tui/src/tui/subscriptions.rs:290`), dialog rows (`jackin-capsule/src/tui/components/dialog/container_info.rs:65-93`), console debug panel (`jackin/src/console/tui/run.rs:317`), startup banner + Enter-gate (`app.rs:126-128,229-262`), plain build streaming (`jackin-image/src/image_build.rs:15`).
  - Container env: `debug_runtime_envs` (`launch_runtime.rs`, verified) injects `JACKIN_DEBUG=1` (+ run id/path) when debug.
- Firehose sites already contained by plan 005 (`cdebug_local!` = file-only): PTY bytes, send-bytes, input events. Remaining high-frequency exported DEBUG sites to demote to TRACE: `frame-geom`/`frame-pane`/`pane scroll frame` (`daemon/compositor.rs:151,160,354`), mouse-motion sites (`daemon/mouse_input.rs:403,224`), host `cockpit-dialog-mouse` (`jackin-launch-tui/src/tui/subscriptions.rs:476-490`), per-dispatch input traces (`daemon/input_dispatch.rs` sites).
- Filter construction single point (post plan 002): `export_filter_directive(level)` + `EXPORT_TARGETS` in `observability.rs`.
- Research-doc contract (target): `JACKIN_TELEMETRY_LEVEL=info|debug|trace`, `JACKIN_TELEMETRY_CATEGORIES=docker,launch,…`, default `info`; remove `--debug`/`JACKIN_DEBUG` as *telemetry* controls; no compat aliases.
- Docs that hard-code the old contract and MUST be amended in this PR: `ENGINEERING.md:64-79` (two-tier rule names `JACKIN_DEBUG` as the telemetry switch), `AGENTS.md` telemetry line, `TESTING.md:65-83` (`--debug` validation flow — stays valid for UI/file purposes; add the telemetry-level note), `docs/content/docs/(public)/guides/run-telemetry.mdx`, `docs/content/docs/reference/runtime/diagnostics.mdx`, `docs/content/docs/(public)/guides/environment-variables.mdx` (new vars). DEPRECATED.md gets a row for "JACKIN_DEBUG as telemetry-export control".

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt/clippy/check | `cargo fmt --check` ; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` ; `cargo check --all-targets --all-features` | exit 0 |
| Tests | `cargo nextest run --all-features` | pass |
| CLI help | `cargo run --bin jackin -- --help` | shows `--telemetry-level`, `--telemetry-category` |

## Scope

**In scope**: `crates/jackin/src/cli.rs`, `crates/jackin/src/app.rs`, `crates/jackin-diagnostics/src/{observability.rs,run.rs,logging.rs,lib.rs}` (level type + gates), `crates/jackin-usage/src/logging.rs` (capsule level), `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs` (env propagation), the firehose demotion sites listed above, tests, and the six doc files + DEPRECATED.md named above.

**Out of scope**: config.toml persistence of these settings (plan 010); UI consumers of `--debug` (unchanged by design); metrics (012); any new categories beyond mapping the existing `debug_log!`/target vocabulary.

## Git workflow

- Propose branch `feat/telemetry-level`; operator confirm. This is a **breaking pre-release change** (PRERELEASE.md permits; changelog entry per its rules). `git commit -s`, push each.

## Steps

### Step 1: The level/category model

New module `crates/jackin-diagnostics/src/telemetry_level.rs` (+ `pub mod` in lib.rs):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum TelemetryLevel { #[default] Info, Debug, Trace }
impl TelemetryLevel {
    pub fn parse(s: &str) -> Option<Self>;          // "info"|"debug"|"trace", ci
    pub fn as_filter_level(self) -> &'static str;    // "info"|"debug"|"trace"
}
#[derive(Debug, Clone, Default)]
pub struct TelemetryConfig { pub level: TelemetryLevel, pub categories: Vec<String> }
pub fn resolve_telemetry_config(cli_level: Option<&str>, cli_categories: &[String]) -> TelemetryConfig
// precedence: CLI flag > JACKIN_TELEMETRY_LEVEL / JACKIN_TELEMETRY_CATEGORIES env > default Info.
```

Global storage: a `OnceLock<TelemetryConfig>` + `pub fn telemetry_config() -> &'static TelemetryConfig` set once in `app.rs` before `RunDiagnostics::start` (mirror `set_debug_mode` placement, `app.rs:90-91`). Capsule side reads env only (`JACKIN_TELEMETRY_LEVEL`/`_CATEGORIES` injected by the host, Step 4).

**Verify**: unit tests for `parse`/precedence (pure fns take env values as params; the env read lives in a thin wrapper — repo pattern).

### Step 2: CLI flags + gate rewiring

1. `cli.rs`: add global `--telemetry-level <info|debug|trace>` (env `JACKIN_TELEMETRY_LEVEL`) and repeatable `--telemetry-category <name>` (env `JACKIN_TELEMETRY_CATEGORIES`, comma-split). Keep `--debug` with help text reworded: "Operator debug UI + local diagnostics firehose. No longer controls telemetry export level (use --telemetry-level)."
2. `observability.rs`: `init_tracing(debug: bool, run_id)` → `init_tracing(config: &TelemetryConfig, run_id)`; level string from `config.level.as_filter_level()`. **Transitional default that preserves current behavior for local files:** `RunDiagnostics::debug` (JSONL firehose capture) and `debug_log!`/`cdebug!` file gates STAY on the `--debug` boolean — the local firehose is an operator-UI concern (ENGINEERING.md two-tier rule, unchanged). Only the EXPORT level moves to `TelemetryLevel`.
3. Category filtering: map categories onto the plan-002 directive. Category → target(s) table as `const CATEGORY_TARGETS: &[(&str, &[&str])]` (e.g. `("docker", &["jackin_docker"])`, `("launch", &["jackin_runtime","jackin_launch_tui"])`, `("terminal", &["jackin_capsule::session","jackin_capsule::client_writer"])`, `("render", &["jackin_capsule::daemon::compositor"])`, `("input", &["jackin_capsule::daemon::mouse_input","jackin_capsule::daemon::input_dispatch"])`, `("usage", &["jackin_usage"])`). Semantics: with no categories, all `EXPORT_TARGETS` get `level`; with categories, non-listed targets cap at `info` while listed ones get the full `level`. Implement inside `export_filter_directive(config)`; keep it a pure function of `(level, categories, internal_flag)` with unit tests.

**Verify**: `cargo run --bin jackin -- --help` shows both flags; directive unit tests cover: no categories, one category at trace, internal flag interplay.

### Step 3: TRACE demotions

tracing has a `trace!` level; OTel maps it to severity TRACE. Demote the exported-DEBUG firehose sites to `tracing`-level TRACE:

- Capsule sites (`compositor.rs:151,160,354`, `mouse_input.rs:403,224`, `input_dispatch.rs` per-dispatch sites): these emit via `cdebug!` → bridge DEBUG. Add `ctrace!` macro (same file/stderr behavior as `cdebug!`, bridge level Trace via plan-003's `BridgeLevel::Trace` — add the variant) and switch these sites.
- Host `cockpit-dialog-mouse` (`subscriptions.rs:476-490`): keep the existing `Moved`-event flood out entirely — plan 011 coalesces it; here just note the interaction (011 owns that file's change; do NOT double-edit — if 011 already landed, skip; else leave as DEBUG and record in README dependency notes). ONLY the capsule sites move in this plan.
- Default export filter is `info`, so TRACE rows exports only when `--telemetry-level trace` (+ category narrowing if given). `JACKIN_DEBUG` alone no longer exports the firehose.

**Verify**: seam test — `ctrace!`-tier event not exported at level info/debug, exported at trace.

### Step 4: Container propagation + banner

1. `launch_runtime.rs::debug_runtime_envs`: rename concern — inject `JACKIN_TELEMETRY_LEVEL={level}` and `JACKIN_TELEMETRY_CATEGORIES={csv}` whenever OTLP propagation is active (beside `OTEL_EXPORTER_OTLP_ENDPOINT`, the verified block), independent of `--debug`; `JACKIN_DEBUG=1` injection stays tied to `--debug` (UI/file firehose in the capsule).
2. `observability.rs::capsule_debug()` → read `JACKIN_TELEMETRY_LEVEL` for the capsule export filter (default info); delete the `JACKIN_DEBUG` read THERE only.
3. Startup banner (`app.rs:229-262`): add one line `telemetry: level=<level> categories=<csv|all>` when OTLP is configured.

**Verify**: `cargo nextest run --all-features`; grep `rg -n "JACKIN_DEBUG" crates/jackin-diagnostics/src/observability.rs` → no matches.

### Step 5: Docs + DEPRECATED + changelog

- ENGINEERING.md telemetry section: two-tier rule text stays for the local file/console tiers; add: "OTLP export level is `--telemetry-level` (info default, debug, trace); categories via `--telemetry-category`; `--debug` no longer widens export."
- TESTING.md: `--debug` validation flow unchanged for local JSONL; add a note that backend-side verbosity needs `--telemetry-level debug|trace`.
- run-telemetry.mdx + environment-variables.mdx: document the two new flags/env vars (env-vars page gets the telemetry subsection — coordinate with plan 013 if it landed first; edit, don't duplicate).
- diagnostics.mdx: level taxonomy table (ERROR/WARN/INFO/DEBUG/TRACE with examples).
- DEPRECATED.md: row — "`JACKIN_DEBUG` as telemetry-export control — removed <date>; use `JACKIN_TELEMETRY_LEVEL`" (per PRERELEASE.md, no shim).

**Verify**: full gate; `bun` docs build NOT required for MDX edits but run `cargo xtask docs repo-links` if available (`cd docs && cargo xtask docs repo-links` per docs/CLAUDE.md) → exit 0.

## Test plan

- Pure: `TelemetryLevel::parse`, precedence resolution, `export_filter_directive(config)` matrix (level×categories×internal).
- Seam: TRACE-tier suppressed at info, exported at trace; DEBUG tier exported at debug not info (existing plan-001 test updated to the config signature).
- CLI: extend `crates/jackin/tests/cli_debug_env.rs` pattern with a `cli_telemetry_env.rs` spawning the binary with `JACKIN_TELEMETRY_LEVEL=trace --help`-level checks (env-var wiring parse test, same approach as the verified existing file).

## Done criteria

- [ ] `--telemetry-level`/`--telemetry-category` in `--help`; env equivalents work (CLI test)
- [ ] `rg -n "if debug \{ \"debug\" \} else \{ \"info\" \}" crates/jackin-diagnostics` → no matches
- [ ] Capsule export level driven by `JACKIN_TELEMETRY_LEVEL` (grep proves no `JACKIN_DEBUG` in observability.rs)
- [ ] Firehose capsule sites at TRACE; seam proves default-info export excludes them
- [ ] ENGINEERING.md / TESTING.md / run-telemetry.mdx / environment-variables.mdx / diagnostics.mdx / DEPRECATED.md all updated in the same PR
- [ ] fmt/clippy/nextest green; `plans/README.md` updated

## STOP conditions

- Plans 002/003/005/006 not all landed (this plan edits their outputs).
- The `BridgeLevel::Trace` addition conflicts with a landed severity design that enumerated levels differently — reconcile, don't fork.
- Rewiring `init_tracing`'s signature cascades beyond `app.rs` + tests (grep callers first: `rg -n "init_tracing" crates/` — expect run.rs + tests only).
- Operator pushes back on removing `JACKIN_DEBUG`-as-export-control in review — that is a product decision recorded in the research doc; link it rather than re-litigating, but STOP if they overrule.

## Maintenance notes

- Plan 010 adds config.toml persistence for level/categories (env/CLI still win).
- Category table is small by design; grow it only when an operator asks to scope a real domain.
- The `--debug` boolean's remaining consumers are all operator-UI + local-file; a future plan could rename it `--debug-ui`, deferred deliberately (churn > value now).
