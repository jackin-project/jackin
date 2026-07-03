# Plan 006: Structured event taxonomy — event names, categories, and fingerprint-stable bodies; console prefixes only at the render boundary

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-diagnostics/src crates/jackin-usage/src crates/jackin-runtime/src/runtime/launch/exit_diagnosis.rs`
> Plans 001–005 legitimately reshaped parts of these files. Re-verify each
> excerpt below against live code; the load-bearing facts are the prefix
> formatting sites and the `otel_keys` module. STOP on contradiction.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/003-error-severity-truth.md, plans/004-route-record-direct-through-tracing.md, plans/005-redaction-boundary.md
- **Category**: tech-debt / bug
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

Exported log records are console strings, not events. The category lives inside the body as a bracket prefix (`[jackin debug docker] inspect …`, `[jackin-capsule] attach …`), so a backend can only filter by text search; and error bodies interpolate container names, uids, io-error strings, and 40-line log tails, so backends that group by body split one recurring failure into N issues (observed live: one attach failure = 14 Parallax issues, keyed by container name/uid/command text). OpenTelemetry's model (spec ≥1.53) gives log records a top-level **EventName**; `opentelemetry-appender-tracing` maps the tracing event's fields to attributes and `message` to the body. This plan makes the exported record: short stable body + `event.name` + `log.category` + facts as attributes — while the console/file line keeps its exact current human formatting, rendered at the boundary instead of baked into the message.

## Current state

Prefix baked before emit:

- Host: `crates/jackin-diagnostics/src/logging.rs:149-151` `format_debug_line` → `"[jackin debug {category}] {message}"`; `emit_debug_line` (`:63-92`) passes the FORMATTED line to `run::active_debug(category, &line)` → `RunDiagnostics::debug` (`run.rs:476-482`) emits kind `debug`, message = formatted line, `detail` = category.
- Capsule: `crates/jackin-usage/src/logging.rs:141-164` — `clog!`/`cdebug!` (and plan-003 `cwarn!`/`cerror!`) format `[jackin-capsule…] {msg}` then `write_line(&line)` AND `bridge_log(level, &line)` — the same prefixed string goes to the file and the bridge.

Key registry already exists — `crates/jackin-diagnostics/src/observability.rs:23-55` `pub mod otel_keys` (dotted keys: `jackin.component`, `parallax.run.id`, `jackin.launch.stage`, `jackin.container.name`, …). Extend it, don't invent a second registry.

High-cardinality error body — `crates/jackin-runtime/src/runtime/launch/exit_diagnosis.rs:155-168` (verified):

```rust
anyhow::anyhow!(
    "capsule attach failed for {container_name}: {err}\ncapsule log: {capsule_log_str}\n{evidence}"
)
```

and `:110-129` builds `"container {container_name} {phase_label} ({reason}); last 40 log lines:\n{text}"`. These strings become log bodies via plan 003/004's error events.

`emit_jsonl_event_with_level` (`observability.rs:1155-1203`, as reshaped by plans 003/004): fields `run_id`, `kind`, `stage?`, `detail?`, `error_type?`; body = message. The `kind` values are already a de-facto event vocabulary (`stage_started`, `container_crash`, `timing_done`, `git_pull`, `auth`, …).

opentelemetry-appender-tracing 0.32 behavior (verified in docs): tracing event fields → attributes; field named `message` → Body; the tracing event *name* → OTel EventName; `target` → InstrumentationScope. Tracing macros support dotted field names when quoted: `tracing::info!("log.category" = %cat, …)` — NO: quoted field-name syntax uses `{ "log.category" = v }`? Correct current syntax: dotted names ARE allowed bare in tracing macros (`event.name = "x"` parses as a field named `event.name` — tracing splits on `=` and accepts dots; the tracing docs' own OTel examples use `otel.name = …`). Trust the `otel.name` precedent already compiling in this repo (`observability.rs:937`, `screen.rs:117`): dotted field names work.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy / check | `cargo fmt --check` ; `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` ; `cargo check --all-targets --all-features` | exit 0 |
| Tests | `cargo nextest run --all-features` | pass |
| Kind inventory | `rg -no 'compact\("([a-z_]+)"' crates/ \| sort -u` | current kind vocabulary |

## Scope

**In scope**:

- `crates/jackin-diagnostics/src/observability.rs` (`otel_keys` + emit fields), `logging.rs`, `run.rs` (debug() signature)
- `crates/jackin-usage/src/logging.rs`, `telemetry.rs` (bridge carries raw message + category, prefix only at `write_line`)
- `crates/jackin-runtime/src/runtime/launch/exit_diagnosis.rs` (stable-body split for the two error constructors)
- Tests: the plan-001 seam files + affected crates
- `docs/content/docs/reference/runtime/diagnostics.mdx` (attribute table)

**Out of scope**:

- Renaming the JSONL FILE fields (`kind`/`stage`/`detail` stay — the file is a consumed contract).
- Spans (007), verbosity levels (008), any call-site "kind" renames beyond the mechanical mapping (the vocabulary cleanup is a Maintenance follow-up).
- Parallax-side fingerprinting (parallax plan 019 consumes what this exports).

## Git workflow

- Propose branch `refactor/telemetry-event-taxonomy`; operator confirm; `git commit -s` per step, push after each.

## Steps

### Step 1: Extend `otel_keys`

Add to `observability.rs::otel_keys` (dotted, documented, one spelling):

```rust
pub const EVENT_NAME: &str = "event.name";       // OTel log EventName mirror
pub const EVENT_OUTCOME: &str = "event.outcome"; // success|failure|expected_close|skipped|cache_hit|cache_miss
pub const LOG_CATEGORY: &str = "log.category";   // debug_log!/clog! category tag
pub const ERROR_TYPE: &str = "error.type";       // semconv: stable machine error kind
pub const OPERATION: &str = "jackin.operation";  // e.g. capsule.attach, image.build
```

**Verify**: `cargo check -p jackin-diagnostics --all-features` → 0.

### Step 2: Emit structure, render prefixes at the boundary (host)

1. `emit_debug_line` (`logging.rs:63`): pass the RAW message down; build the prefixed line only for the stderr/buffer branches (`should_tee_debug_to_stderr` path and the no-run fallback). Change `run::active_debug(category, &line)` to `active_debug(category, message /* raw */)`.
2. `RunDiagnostics::debug` (`run.rs:476-482`): emit with `stage=None`, `detail=Some(category)` REPLACED by the category in a dedicated field: extend `emit_jsonl_event_with_level` with `category: Option<&str>` emitted as field `log.category`. For the JSONL file, keep writing the category into `detail` (file contract unchanged) — the layer visitor maps `log.category` → detail when detail is absent (adjust `DiagnosticsEventVisitor::record_owned`, `observability.rs:153-163`).
3. The JSONL file's `message` for debug events currently contains the prefixed line; after this step it becomes the raw message. THIS CHANGES FILE CONTENT for debug events. Check consumers: `summarize_reader` (`summary.rs`) — grep for `[jackin debug` in `summary.rs` and `crates/jackin-xtask/src/pty_fixture.rs`; if either parses the prefix, STOP (see STOP conditions). Existing test `tests.rs:403-411` pins `format_debug_line` — keep that fn and test (still used at the render boundary).
4. `emit_jsonl_event_with_level`: add `event.name` mirroring `kind` (field `event.name = kind`) so backends get the standard key without a JSONL file change.

**Verify**: seam tests — exported debug record: body == raw message (no `[jackin debug`), attrs `log.category="docker"`, `event.name="debug"`. `cargo nextest run -p jackin-diagnostics --all-features` → pass (update plan-001 pins).

### Step 3: Same split in the capsule tier

In `jackin-usage/logging.rs`, macros currently format then pass the SAME line to file and bridge. Change each macro body to:

```rust
let raw = format!($($arg)*);
$crate::logging::write_line_tagged(TIER_PREFIX, &raw);       // file/stderr keeps exact current format
$crate::telemetry::bridge_log(LEVEL, &raw);                   // bridge gets clean body
```

where `write_line_tagged(prefix, raw)` formats `"{prefix} {raw}"` and delegates to `write_line` (keep `write_line` public for the panic hook). No behavior change for `multiplexer.log`/stderr (byte-identical lines); the bridged body loses the prefix.

**Verify**: `rg -n '\[jackin-capsule' crates/jackin-usage/src/telemetry.rs` → no matches; capsule tests pass.

### Step 4: Stable bodies for the attach/exit failures

`exit_diagnosis.rs` — split human message from telemetry facts:

1. `attach_failure_error` (`:155-168`): keep returning the rich `anyhow::Error` for the OPERATOR (unchanged rendering), but stop using its full string as the telemetry body. At the emit site where this error becomes telemetry (post-plan-003 the `run.error(...)` call; locate via `rg -n "attach_failure_error" crates/jackin-runtime`), emit instead: body `"capsule attach failed"`, `error_type="attach_error"`, `jackin.operation="capsule.attach"` (new field param or via `detail` JSON — prefer explicit fields added to the emit signature in Step 1's extension), `jackin.container.name={container_name}`, evidence via the plan-005 `redact_and_cap` as `detail`.
2. `diagnose_with_state` container-exit bodies (`:110-129`): the `container_crash` event (already structured via `container_exited`, `run.rs:516-551`) is fine; ensure the EVENT body is `"container crashed"` / `"container exited"` constants with facts in the detail JSON (adjust `container_exited`'s `msg` strings `run.rs:536-540`: move `{container_name}` out of the message into the already-present detail JSON; message becomes `"container OOM killed" | "container exited"` — the stage field already carries the container name).

Rule to encode in comments: **exported bodies never interpolate identifiers**; identifiers ride attributes (`otel_keys`).

**Verify**: seam test `attach_failure_body_is_stable` — two emissions with different container names produce identical bodies and differing `jackin.container.name` attrs.

### Step 5: Docs + full gate

`reference/runtime/diagnostics.mdx`: add/refresh the attribute table (`event.name`, `event.outcome`, `log.category`, `error.type`, `jackin.operation`, existing keys), and the "bodies are stable; identifiers are attributes" rule.

**Verify**: fmt / clippy / `cargo nextest run --all-features` → exit 0.

## Test plan

- Seam: debug-record shape (step 2), capsule bridged body without prefix (unit: `bridge_log` input assertion via test-subscriber), stable attach body (step 4), `event.name` mirrors kind for `stage_started`/`container_crash`.
- File-contract: existing `tests.rs` JSONL tests updated ONLY where debug-message content changed (assert raw message now; add comment).
- Pure: `write_line_tagged` formatting == old `clog!` output (byte compare against `"[jackin-capsule] x"`).

## Done criteria

- [ ] Seam: no exported body contains `[jackin debug` or `[jackin-capsule` (test greps emitted logs)
- [ ] `multiplexer.log` line format byte-identical (unit test on `write_line_tagged`)
- [ ] `event.name`, `log.category` attributes present on exported records (seam tests)
- [ ] Attach-failure export body constant across container names (test)
- [ ] fmt/clippy/nextest all green; `plans/README.md` updated

## STOP conditions

- `summary.rs` or `pty_fixture.rs` parses the `[jackin debug …]` prefix out of JSONL `message` (grep first). If so, report — the file-side change needs a coordinated fixture/summary update this plan hasn't scoped.
- Dotted field names fail to compile in `tracing::event!` for `event.name`/`log.category` (contrary to the `otel.name` precedent) — fall back to underscore names ONLY for the tracing field and note that the wire keys diverge from `otel_keys`; then STOP and report (that divergence needs a decision, not improvisation).
- Any operator-facing string changes (stderr/TUI lines) — this plan must be invisible on the console.

## Maintenance notes

- Follow-up vocabulary pass (deferred): normalize `kind` values into a documented `event.name` catalog (`run.*`, `launch.stage.*`, `container.*`, `credentials.*`, `image.build.*`, `capsule.attach.*`, `cleanup.*`) and add `event.outcome` at the emit sites that know success/failure — mechanical once this structure lands.
- Parallax plan 019 (fingerprint v2) assumes: stable body + `error.type` + `jackin.operation` + container name as attribute. If you rename any of those keys, update that plan.
- Reviewer scrutiny: no call site should build `format!("... {container_name} ...")` bodies for exported events anymore; flag new ones.
