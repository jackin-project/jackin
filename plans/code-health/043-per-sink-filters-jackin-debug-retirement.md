# Plan 043: Per-sink telemetry filters; retire `JACKIN_DEBUG` as a telemetry control

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat fabe88406..HEAD -- crates/jackin-diagnostics/src/observability.rs crates/jackin-diagnostics/src/logging.rs crates/jackin-usage/src/logging.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition. Plan 018 Step 1 (single OTLP
> builder) landing IS expected drift — its refactor is this plan's substrate;
> read its diff and continue.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (behavior-changing across host + capsule debug plumbing; mitigated by the alias shim keeping `JACKIN_DEBUG=1` working)
- **Depends on**: plans/code-health/018-telemetry-drift-proofing.md (Step 1's single provider-builder is where the per-sink filters attach; doing this before 018 doubles the work)
- **Category**: tech-debt
- **Planned at**: commit `fabe88406`, 2026-07-09

## Why this matters

Roadmap Phase 8 item 2: "give each sink — console, diagnostics file, OTLP logs, OTLP spans — its own filter so console compactness and backend richness stop being one knob," and "finish retiring the binary `--debug`/`JACKIN_DEBUG` as a telemetry control." The level/category model already shipped (`JACKIN_TELEMETRY_LEVEL`, `JACKIN_TELEMETRY_CATEGORIES`, `JACKIN_OTEL_INTERNAL` all parse and gate today), but (a) one `EnvFilter` directive is cloned onto both the OTLP span layer and the OTLP log layer — they cannot diverge — and (b) `JACKIN_DEBUG` is still read directly in ~19 production files, so two overlapping controls decide the same questions. This plan finishes the model: per-sink level resolution, and `JACKIN_DEBUG` reduced to a single alias shim.

## Current state

All excerpts verified by direct read at `fabe88406`.

- One directive, two layers — host `observability.rs:724-728`:
  ```rust
  let directive = export_filter_directive(export_level(debug));
  let installed = tracing_subscriber::registry()
      .with(JackinDiagnosticsLayer)
      .with(span_layer.with_filter(EnvFilter::new(directive.clone())))
      .with(log_layer.with_filter(EnvFilter::new(directive)))
  ```
  Capsule identical shape at `:805` + `:820-823` (plus the fixed `otlp_diag_layer` stderr filter :814-819, which is already per-sink and stays).
- Level resolution shipped: `export_level(debug)` (`observability.rs:887-893`) → `crate::telemetry_level(debug)`; `telemetry_level` (`crates/jackin-diagnostics/src/logging.rs:34-46`) resolves `JACKIN_TELEMETRY_LEVEL` env → config → `--debug` fallback. Categories + wildcard + config backing: `logging.rs:97-137` (`debug_capture_enabled`, `telemetry_category_enabled`, `set_config_telemetry`). `JACKIN_OTEL_INTERNAL`: `observability.rs:879-885`. `EXPORT_TARGETS` allowlist: `observability.rs:851-877`.
- Capsule-side duplicate level parsing: `crates/jackin-usage/src/logging.rs:84-101` — `init()` re-parses `JACKIN_TELEMETRY_LEVEL` and `JACKIN_DEBUG` truthiness itself (`DEBUG_ENABLED`/`TRACE_ENABLED` atomics); `capsule_debug()` (`observability.rs:839-846`) parses `JACKIN_DEBUG` truthiness a third time.
- `JACKIN_DEBUG` production readers (19 files; from `grep -rln JACKIN_DEBUG crates/ --include='*.rs'` minus tests): key ones — `crates/jackin/src/cli.rs` (3 hits; the `--debug` flag plumbing), `crates/jackin-diagnostics/src/observability.rs` (`capsule_debug`), `crates/jackin-usage/src/logging.rs` (capsule init), `crates/jackin-runtime/src/runtime/launch/launch_runtime.rs` + `apple_container.rs` + `image.rs` (env injection into containers/builds), `crates/jackin-capsule/src/{session,runtime_setup,daemon/control,daemon/compositor,daemon/input_dispatch}.rs`, `crates/jackin-usage/src/usage/codex.rs`, `crates/jackin-tui-lookbook/src/stories.rs`. Enumerate the live list yourself in Step 3 — counts drift.
- Two-tier contract (ENGINEERING.md + `crates/jackin-diagnostics/AGENTS.md`): `cdebug!` firehose "gated on `JACKIN_DEBUG=1`" — prose that must be updated to name `JACKIN_TELEMETRY_LEVEL` once the shim lands.
- The dossier (docs/content/docs/reference/research/agent-telemetry/parallax-observability-findings.mdx, "CLI And Environment Contract"): desired end state is `JACKIN_TELEMETRY_LEVEL`/`_CATEGORIES` only, no compat aliases long-term; pre-release policy (PRERELEASE.md) allows the break, `DEPRECATED.md` records it.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Diagnostics tests | `cargo nextest run -p jackin-diagnostics` | all pass |
| Capsule closure | `cargo nextest run -p jackin-usage -p jackin-capsule` | all pass |
| Runtime + cli tests | `cargo nextest run -p jackin-runtime -p jackin` | all pass |
| Workspace clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-diagnostics/src/observability.rs` (+ `observability/otlp/tests.rs`) — per-sink level resolution + split filters
- `crates/jackin-diagnostics/src/logging.rs` (+ tests) — sink-level helper, shim
- `crates/jackin-usage/src/logging.rs` (+ tests) — capsule init reads the shared resolution
- The `JACKIN_DEBUG` reader files enumerated in Step 3 (mechanical swap to the shared helpers)
- `DEPRECATED.md`, `ENGINEERING.md`, `crates/jackin-diagnostics/AGENTS.md` two-tier prose, `TESTING.md` env-var mentions
- Roadmap Phase 8 item 2 status
- Docs pages documenting `--debug`/`JACKIN_DEBUG` (grep `docs/content/docs` for both; update operator-facing wording to `JACKIN_TELEMETRY_LEVEL`)

**Out of scope** (do NOT touch):
- Removing the `--debug` CLI flag itself — it stays and maps to level=debug (operator ergonomics; the roadmap retires it as *the telemetry control*, not as a flag).
- The console rich-surface buffering logic (`should_tee_debug_to_stderr`, debug buffer) — untouched; it is already an independent console-sink policy.
- The macro stacks and their call sites (041's waves), the metric instruments (042), the conformance lane (044).
- `JACKIN_TELEMETRY_INTERNAL` naming — `JACKIN_OTEL_INTERNAL` already ships; keep it (renaming wire-visible env is needless churn; note the dossier divergence in the PR body).

## Git workflow

- Branch off `main`: `refactor/telemetry-per-sink-filters`.
- Conventional Commits, `-s`, push per commit. PR to `main`; do not merge. Capsule closure touched → capsule smoke block verbatim in the PR body.

## Steps

### Step 1: Per-sink level resolution

In `logging.rs`, add:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetrySink { OtlpSpans, OtlpLogs, Console, DiagnosticsFile }

pub fn sink_level(sink: TelemetrySink, debug: bool) -> TelemetryLevel
```

Resolution order per sink: `JACKIN_TELEMETRY_<SINK>_LEVEL` env override (`JACKIN_TELEMETRY_OTLP_SPANS_LEVEL`, `JACKIN_TELEMETRY_OTLP_LOGS_LEVEL`, `JACKIN_TELEMETRY_CONSOLE_LEVEL`, `JACKIN_TELEMETRY_FILE_LEVEL`) → global `telemetry_level(debug)` (existing fn, unchanged semantics). Reuse `parse_telemetry_level`; unit-test the override/fallback matrix in `logging/tests.rs` (env-dependent tests must use the existing `DIAGNOSTICS_TEST_LOCK` — see `lib.rs:58-59` — and set/remove vars inside the lock; follow how `debug_capture_enabled_with_env` (:104) avoids env instead where possible by taking params).

**Verify**: `cargo nextest run -p jackin-diagnostics` → pass with new matrix tests.

### Step 2: Split the OTLP layer filters

In `observability.rs` (post-018 this is inside the single provider-builder's caller): replace the cloned directive with per-sink directives — `export_filter_directive(level_for(OtlpSpans))` on the span layer, `export_filter_directive(level_for(OtlpLogs))` on the log layer, in both `init` and `init_capsule` (or the one unified site if 018 landed). The `otlp_diag_layer` stderr filter stays fixed. Extend `otlp/tests.rs`: with `JACKIN_TELEMETRY_OTLP_SPANS_LEVEL=info` and `JACKIN_TELEMETRY_OTLP_LOGS_LEVEL=trace` (or the param-injected equivalent — prefer refactoring `export_filter_directive`/`export_level` to accept the level so tests need no env), a debug-level event is captured by the log exporter and NOT by the span exporter.

**Verify**: `cargo nextest run -p jackin-diagnostics` → pass; the new divergence test proves the knobs are independent.

### Step 3: Retire `JACKIN_DEBUG` to one alias shim

1. Enumerate: `grep -rln "JACKIN_DEBUG" crates/ --include="*.rs" | grep -v tests` — list every file in the PR body.
2. The shim: exactly ONE place resolves `JACKIN_DEBUG` truthiness as an alias for `JACKIN_TELEMETRY_LEVEL=debug` when the latter is unset — the natural home is `telemetry_level()` (`logging.rs:34-46`): add the `JACKIN_DEBUG` check between env-level and config-level. Capsule side: `jackin-usage/src/logging.rs:84-101`'s hand parsing collapses to calling the shared resolution (jackin-usage depends on jackin-diagnostics? **Check `crates/jackin-usage/Cargo.toml` first** — the re-export direction today is diagnostics→usage for macros; if usage cannot depend on diagnostics, replicate the 3-line shim locally with a comment naming the canonical copy, and say so in the PR body). `capsule_debug()` (`observability.rs:839`) collapses into the same resolution.
3. Mechanical swap: every other reader switches to `is_debug_mode()` / `telemetry_level(...)` / `sink_level(...)` as appropriate. The runtime env-injection sites (`launch_runtime.rs`, `apple_container.rs`, `image.rs`) switch from injecting `JACKIN_DEBUG=1` into containers to injecting `JACKIN_TELEMETRY_LEVEL=<resolved level>` — BUT keep also injecting `JACKIN_DEBUG` for one release so an older capsule image still honors the operator's `--debug` (pre-release skew tolerance; note it in DEPRECATED.md for removal).
4. `DEPRECATED.md`: entry for `JACKIN_DEBUG` as telemetry control (alias only; removal after capsule-image skew window). ENGINEERING.md + diagnostics AGENTS two-tier prose: `cdebug!` firehose gated on `JACKIN_TELEMETRY_LEVEL=debug` (alias `JACKIN_DEBUG=1`).

**Verify**: `grep -rln "JACKIN_DEBUG" crates/ --include="*.rs" | grep -v tests` → only the shim file(s) + the deliberate dual-injection sites remain (list them); `cargo nextest run -p jackin-usage -p jackin-capsule -p jackin-runtime -p jackin` → all pass.

### Step 4: Docs + gates

Update operator docs mentioning `--debug`/`JACKIN_DEBUG` for telemetry verbosity (grep `docs/content/docs -r` for both strings; keep the `--debug` flag documented, describe levels via `JACKIN_TELEMETRY_LEVEL`). Roadmap Phase 8 item 2 → shipped note (per-sink + retirement), with the `JACKIN_OTEL_INTERNAL`-naming divergence recorded.

**Verify**: `cargo xtask docs repo-links && cargo xtask roadmap audit` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- New: sink_level override/fallback matrix (Step 1); span-vs-log filter divergence via the in-memory rig (Step 2); shim test — `JACKIN_DEBUG=1` with `JACKIN_TELEMETRY_LEVEL` unset yields `Debug`, with `=info` set yields `Info` (env-locked).
- Regression: the four crate suites above; `cli_debug_env.rs` integration test in `crates/jackin/tests/` (13 hits — it exercises the `--debug` env plumbing; update its expectations deliberately, never delete assertions).

## Done criteria

- [ ] Span and log OTLP layers filter independently (divergence test green)
- [ ] `sink_level` exists with 4 sinks + env overrides, tested
- [ ] `JACKIN_DEBUG` read in exactly one resolution site (+ documented dual-injection sites); all other readers use shared helpers
- [ ] Container env injection sends `JACKIN_TELEMETRY_LEVEL` (plus temporary `JACKIN_DEBUG` alias, recorded in DEPRECATED.md)
- [ ] ENGINEERING.md / AGENTS / TESTING.md / operator docs / roadmap item 2 updated
- [ ] All four suites + clippy green; `cargo xtask ci --fast` → `ci gate OK`
- [ ] `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- 018 has not landed AND the two init fns have diverged from the excerpts (double-filter work would be built twice — report and wait for 018).
- `crates/jackin/tests/cli_debug_env.rs` encodes an operator contract that the shim would silently change (e.g. `JACKIN_DEBUG=0` overriding a config-set debug level) — report the exact semantics before choosing.
- The capsule-image skew check reveals the capsule reads `JACKIN_DEBUG` at a site that cannot fall back (older host injecting only the new var into a newer capsule is fine; the reverse must keep working via the dual injection — if a path cannot, report it).
- Any sink's filter change would alter what `multiplexer.log` captures by default (operators tail it for bug reports) — file-sink default must stay behaviorally identical at default levels.

## Maintenance notes

- Plan 044's conformance lane should pin the per-sink defaults (a scenario asserting console-quiet + backend-rich under `--debug`).
- The dual `JACKIN_DEBUG` injection is removable once the capsule image floor moves past this release — DEPRECATED.md entry is the tracker.
- Reviewer scrutiny: Step 3's mechanical swap must not change *decisions* (each reader's question — "am I verbose?" — must resolve identically under default envs); the Step 2 env-free refactor (levels as params) is preferred over env-locked tests.
