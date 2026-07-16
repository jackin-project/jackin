# Plan 011: Classify and migrate every legacy logging call site — `debug_log!`, capsule macros, diagnostic prints

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- crates/jackin-core/src/debug_log.rs crates/jackin-usage/src/logging.rs crates/jackin-usage/src/telemetry.rs crates/jackin-diagnostics/src/logging.rs crates/jackin-diagnostics/src/debug_log_adapter.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition. (Call-site counts WILL have drifted
> — regenerate the census in step 1; the counts below are the planning-time
> baseline, not a contract.)

## Status

- **Priority**: P1
- **Effort**: L (the largest mechanical surface of the program: ~278 `debug_log!` + ~300 capsule-macro + selected `eprintln!` sites)
- **Risk**: MED (high touch count, low per-site risk; classification errors lose triage signal)
- **Depends on**: plans/unified-otel-observability/004-telemetry-facade-api.md, 008-execution-boundaries.md, 009-tui-screens-actions.md, 010-capsule-cycles-agents-jobs.md
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements the "Raw `debug_log!`, Capsule prefix macros, and ungoverned `tracing` → each call classified as typed operator output or a registered span/log/metric; no raw emission bypass" row of "Required jackin❯ end state", honoring the workspace-inventory note that "implementation must classify each site rather than replace calls mechanically"; the roadmap item is the binding contract and overrides this plan on any conflict.
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

The 2026-07-15 inventory found 284 `debug_log!` occurrences, ~300 capsule prefix-macro occurrences, and 127 `eprintln!` occurrences. These are the ungoverned emission paths the unified model forbids: they disagree on identity, redaction, names, and failure behavior. After plans 008–010, the shared boundaries already emit the governed signal for most of what these lines narrate — so the majority of sites become deletions or DEBUG-tier facade events, not inventions. This plan classifies every site, migrates or deletes it, and removes the legacy macro machinery so no raw bypass remains.

## Current state

(planning-time baseline; regenerate before working)

- **`debug_log!`** (`crates/jackin-core/src/debug_log.rs:71-78`; port trait `DebugLogSink` `:25-30`; sink installed by `jackin_diagnostics::install_debug_log_sink`, `lib.rs:27-28` + `debug_log_adapter.rs`). Call sites (production, no tests): `jackin-runtime` 125, `jackin-isolation` 46, `jackin-docker` 27, `jackin-image` 22, `jackin-console` 21, `jackin` 18, `jackin-instance` 12, `jackin-host` 3, `jackin-config` 2, `jackin-launch-tui` 1, `jackin-console-oppicker` 1 (+4 def/docs in `jackin-core`, +2 doc/re-export in `jackin-diagnostics`). Hotspots: `jackin-runtime/src/runtime/host_attach.rs` (25), `jackin-isolation/src/materialize.rs` (27), `jackin-docker/src/docker_client.rs` (23), `launch_runtime.rs` (16).
- **Capsule macros** (`crates/jackin-usage/src/logging.rs`): `clog!` `:216` (171 sites), `cdebug!` `:231` (115), `ctrace_payload!` `:245` (12), `cwarn!` `:266` (2), `cerror!` `:275` (3), `cdebug_local!` `:259` (0). Each writes `[jackin-capsule…]`-prefixed stderr+file lines AND bridges to OTLP via `telemetry::bridge_log` (`telemetry.rs:134-216`, event names `capsule.log/.debug/.warn/.error/.trace`).
- **Diagnostic `eprintln!`** (distinct from operator output): `jackin-runtime/src/runtime/cleanup.rs` (~17, cleanup-failure warnings), `jackin-instance/src/auth.rs` (~9, auth warnings). Most other `println!/eprintln!` is INTENTIONAL operator output under `#![expect(clippy::print_stdout/print_stderr)]` (e.g. `cli/status.rs`, `app/config_cmd.rs`) — NOT in scope for conversion; only diagnostic-flavored prints are.
- **ENGINEERING.md** (lines 64-81) currently mandates the `clog!`/`cdebug!` two-tier contract — plan 015 rewrites it; this plan's classification implements the successor: compact tier → INFO events, firehose tier → DEBUG events gated by governed level, payload traces → TRACE behind the explicit gate.
- Facade + boundaries available (plans 004/008/009/010): operations own outcomes/errors; lower layers enrich spans instead of re-logging (contract: "Lower layers return typed errors and enrich spans rather than repeating ERROR at every level"; "only the operation owner emits the ERROR log").
- Lint from plan 004 step 6 bans raw `tracing` outside the telemetry crates with a shrink-only allowlist (raw-tracing sites: jackin-runtime 11, jackin-instance 3 events + `usage.rs:423`/`launch_pipeline.rs:349` `#[instrument]`) — this plan drains those too.

## Classification rulebook (apply per site, in order)

1. **Duplicate of a governed signal** (the same fact now emitted by a plan 008–010 boundary span/event/metric — e.g. `docker_client.rs` request narrations, launch stage chatter, attach handshake outcomes): **DELETE**.
2. **Semantic lifecycle/state change not yet covered** (daemon start, session spawn/exit already covered; e.g. config fallback decisions, isolation materialization milestones): **registered INFO event** via the facade — add the event to the schema registry FIRST (closed-set discipline; batch new event names per crate into one registry commit).
3. **Handled degradation / recovered retry**: **WARN event** (bounded reason field, no raw error string as a metric dim).
4. **Terminal failure narration**: does an operation own this failure? If yes, the guard/`error.type` already records it — DELETE the line (or convert to span enrichment via typed error context). If it is genuinely the final failing owner and unowned, wrap in an operation instead of logging loose (rare — flag these).
5. **Bounded diagnostic decision detail** (the `cdebug!` firehose: parser events, dispatch traces, per-write notes): **DEBUG event** with typed fields; name per subsystem (e.g. `capsule.input.dispatch`), body ≤ contract limits. Anything >~10/min in normal operation must be DEBUG, never INFO (existing ENGINEERING.md ratio rule carries over).
6. **Raw payload dumps** (`ctrace_payload!` PTY bytes/frames): TRACE is structured and privacy-bound — raw PTY/terminal content is PROHIBITED as telemetry. These sites convert to TRACE events with bounded structural fields (byte counts, parse classifications) — never content. The render-conformance FIXTURE recording path (raw bytes to a local file for test extraction, `TESTING.md:88-102`) is TEST tooling, stays outside production telemetry, and is re-anchored in plan 013 — do not delete the capture capability here; keep it behind its explicit fixture-recording gate.
7. **Operator-facing information** (a human must see it on the terminal regardless of telemetry): route through the typed operator-output port (`jackin_core::OperatorNoticeSink` / `emit_operator_notice`, `jackin-diagnostics/src/logging.rs:268` — the port survives; telemetry never renders there).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Census (regenerate first) | `grep -rn "debug_log!\|clog!\|cdebug!\|ctrace_payload!\|cwarn!\|cerror!" crates/ --include='*.rs' \| grep -v "tests.rs\|macro_rules\|jackin-xtask" \| awk -F: '{print $1}' \| sort \| uniq -c \| sort -rn` | per-file counts |
| Per-crate tests while migrating | `cargo nextest run -p <crate> --locked` | all pass |
| Workspace | `cargo nextest run --workspace --all-features --locked` | all pass |
| Lint gates | `cargo xtask telemetry-registry` and `cargo xtask ci --only lint` | exit 0 |
| Capsule render conformance | `cargo nextest run -p jackin-capsule --locked -E 'test(/conformance/)'` | all pass |

## Scope

**In scope:**
- Every production `debug_log!` / `clog!` / `cdebug!` / `ctrace_payload!` / `cwarn!` / `cerror!` call site (census-driven), across: `jackin-runtime`, `jackin-isolation`, `jackin-docker`, `jackin-image`, `jackin-console`, `jackin`, `jackin-instance`, `jackin-host`, `jackin-config`, `jackin-launch-tui`, `jackin-console-oppicker`, `jackin-capsule`, `jackin-usage`, `jackin-term`, `jackin-core`
- Diagnostic `eprintln!` in `runtime/cleanup.rs` and `jackin-instance/src/auth.rs` (classify per rulebook)
- Raw-`tracing` allowlist drain: `jackin-runtime` (11), `jackin-instance` (3 + instrument sites), `jackin-usage` (`usage.rs:423`, refresh span) — replace with facade operations/events; `launch_pipeline.rs:349` `#[instrument]` replaced by plan 008's launch spans (delete)
- Macro machinery removal once call sites are zero: `crates/jackin-core/src/debug_log.rs` (macro + sink + `is_debug_mode`), `crates/jackin-diagnostics/src/debug_log_adapter.rs` + `install_debug_log_sink`, capsule macros in `crates/jackin-usage/src/logging.rs` + `bridge_log`/`bridge_log_structured`/`BridgeLevel` in `telemetry.rs` (`logging::init`'s file handling survives until plan 013 removes the file itself — after this plan, nothing writes through the macros, so `write_line` callers are only banner/panic paths; migrate those too and leave `logging.rs` as the thin init that plan 013 deletes)
- Lint hardening: the raw-emission allowlists (plans 004/005) plus a new ban on `debug_log!`/capsule-macro tokens once deleted (trivially: they no longer compile — the lint ban prevents re-introduction by name)

**Out of scope:**
- Operator-output `println!/eprintln!` under `#![expect(print_stdout/print_stderr)]` — legitimate product output.
- File/reader/env removal (`multiplexer.log`, JSONL, `JACKIN_TELEMETRY_*` env surface) — plan 013.
- `jackin-dev`, `jackin-xtask` (its 1 `clog!` occurrence is in fixture tooling — verify and leave), tests, benches.

## Git workflow

- Branch: `feature/unified-otel-observability` — single branch, single PR for the whole roadmap item (plans 001–015 together); no per-plan branch or separate PR (operator directive overrides the roadmap's multiple-PR allowance). This plan is intentionally COMMIT-PER-CRATE (e.g. `refactor(runtime): migrate debug_log sites to governed telemetry`) so review and bisection stay possible inside the single PR. Sign `-s`, push after every commit.

## Steps

### Step 1: Census + classification worksheet

Regenerate the census (command table). Produce `plans/unified-otel-observability/worksheets/011-census.md` (commit it): one row per file — path, count per macro, rulebook class guesses. This worksheet is working state for the executor and the reviewer; classes are confirmed during migration.

**Verify**: worksheet exists; totals match the census output.

### Step 2: Migrate crate-by-crate (dependency order: leaf crates first)

Suggested order (small→large, leaf→hub): `jackin-config`, `jackin-console-oppicker`, `jackin-launch-tui`, `jackin-host`, `jackin-instance`, `jackin-image`, `jackin-docker`, `jackin-isolation`, `jackin-console`, `jackin`, `jackin-usage`, `jackin-capsule`, `jackin-runtime`. For each crate: apply the rulebook per site; batch any new event names into the schema registry first; run that crate's tests; commit; push. Deletion-heavy expectations from the boundary overlap: `jackin-docker` (rule 1 for most of its 27 — the Docker decorator covers them), `jackin-runtime` launch/attach chatter (rules 1/5), `jackin-isolation/materialize.rs` (mostly rule 5 DEBUG events — mount paths are PROHIBITED fields; use bounded step names/counts), capsule dispatch firehose (rule 5), capsule payload traces (rule 6).

**Verify** (per crate): `cargo nextest run -p <crate> --locked` → pass; census re-run shows the crate at zero.

### Step 3: Remove the macro machinery

When the census is zero everywhere: delete `debug_log` macro/module + adapter + `install_debug_log_sink`; delete the six capsule macros + `BridgeLevel`/`bridge_log*`; update `crates/jackin-usage/src/lib.rs` and `crates/jackin-capsule/src/lib.rs:43-58` re-exports; remove `jackin-diagnostics` re-export (`pub use jackin_core::debug_log`, `lib.rs:24`). `is_debug_mode` consumers outside logging (debug-chip UI at `crates/jackin/src/console/tui/run.rs:244,295,420,702` and similar) switch to `jackin_diagnostics::is_debug_mode` (the logging.rs atomic, which survives — `--debug` remains operator troubleshooting per the contract).

**Verify**: `grep -rn "debug_log!\|clog!\|cdebug!\|ctrace_payload!\|cwarn!\|cerror!\|bridge_log\|DebugLogSink" crates/ --include='*.rs' | grep -v tests` → empty; `cargo check --workspace --all-targets --locked` → exit 0.

### Step 4: Drain the raw-tracing allowlist

Replace the inventoried raw `tracing::*` production sites with facade calls (or delete where boundaries cover them); shrink the plan 004 lint allowlist to telemetry-crate internals only.

**Verify**: `cargo xtask telemetry-registry` → exit 0 with the allowlist at its minimum; `cargo nextest run --workspace --all-features --locked` → pass.

### Step 5: Volume sanity

The firehose must stay gated: run the volume check (export-volume conformance) and confirm default-mode signal counts stay within the ratchet; DEBUG/TRACE events must not appear in default-mode export (level gating from plan 003).

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features -E 'test(conformance_export_volume)'` → pass without ratchet bumps beyond the reviewed deltas of plans 007–010.

## Reopened audit additions (2026-07-16)

- For every migrated failure site, prove the operation owner is the only ERROR emitter. Deadline/dependency cancellation is an error, expected operator cancellation is not, guard abandonment is an instrumentation fault, and recovered degradation emits one governed WARN.

## Test plan

- Per-crate: existing suites are the regression net (these lines are narration; behavior must not change). Add targeted tests only where a site converts to a NEW registered event with meaningful fields (isolation materialization events, cleanup-failure WARN events).
- Privacy negatives: isolation events carry no mount path; cleanup warnings carry no container name (bounded `error.type` + step name only).
- Grep-based done checks are the primary structural verification (see Done criteria).

## Done criteria

- [ ] Census greps return zero production sites for all six macros and `debug_log!`
- [ ] Macro machinery deleted; `cargo check --workspace --all-targets --locked` exits 0
- [ ] `cargo nextest run --workspace --all-features --locked` exits 0
- [ ] `cargo xtask telemetry-registry` exits 0 with minimal allowlists
- [ ] Export-volume ratchet green without unexplained growth
- [ ] Worksheet committed; `plans/unified-otel-observability/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:
- A site's information has NO rulebook home (not covered by a boundary, not a valid registered event, not operator output) — collect these and report as a batch; they may indicate a missing contract row, which requires the roadmap-first extension process.
- The render-conformance fixture path stops working when `ctrace_payload!` sites convert (TESTING.md:88-102 flow) — the fixture capture must keep functioning until plan 013 re-anchors it.
- Census drift exceeds ±15% from the baseline (substantial parallel development — re-plan the worksheet before proceeding).
- Any migration would change product behavior (a macro call with side effects in its arguments — watch for `format!` args that call functions).

## Maintenance notes

- After this plan, the ONLY emission paths are the facade and the operator-output port — the lint keeps it that way.
- ENGINEERING.md's telemetry section describes the old tiers until plan 015 rewrites it; contributors mid-transition should follow this plan's rulebook.
- Reviewer focus: deletions justified by boundary coverage should name the covering signal in the commit/PR description (spot-checkable); DEBUG-tier volume ratio (~10/min rule) preserved.
