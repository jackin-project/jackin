# Plan 012: `jackin diagnostics validate` — the delivery check — plus typed health over the daemon protocol

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- crates/jackin/src/cli/diagnostics.rs crates/jackin-runtime/src/host_daemon.rs crates/jackin-diagnostics/src/observability.rs crates/jackin/src/app/daemon_cmd.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: plans/unified-otel-observability/002-otlp-composition-root.md, 004-telemetry-facade-api.md, 006-cross-process-propagation.md, 007-identity-lifecycle-roots.md
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements "`jackin diagnostics validate` is the only command that promises a delivery check" and "Persistent daemons expose a sanitized configuration fingerprint and typed current health over the control protocol" from "Direct OTLP runtime contract"; the roadmap item is the binding contract and overrides this plan on any conflict.
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

With local telemetry files gone (plan 013), operators need exactly one honest answer to "is telemetry reaching Parallax?". The contract gives them `jackin diagnostics validate`: emit one marked log, trace, and metric; force-flush; fail when configuration or delivery cannot be confirmed. Persistent daemons additionally expose a sanitized config fingerprint and typed health over their control protocol so a client can inspect a running process without replacing its subscriber. Health reports only jackin❯-owned observations (active signals, outer export attempt/success/failure, facade rejection, flush, shutdown) — never parsed exporter text or SDK-internal queue/retry/partial-success counts.

## Current state

(verified at planning commit)

- `crates/jackin/src/cli/diagnostics.rs:15-21` — `DiagnosticsCommand` has exactly `Summary(DiagnosticsSummaryArgs)` and `Compare(DiagnosticsCompareArgs)` (both read run JSONL files; both are REMOVED by plan 013). This plan ADDS `Validate`; removal of the others stays in 013 so the two plans can land in either order relative to each other's PRs (this one first is recommended — validate must exist before summary/compare disappear).
- Typed health exists after plan 002 (`observability/health.rs`, `TelemetryHealth`: active signals, export attempt/success/failure per signal, facade rejections from plan 004, flush/shutdown results, plus plan 010's `capsule_export` gap field).
- Endpoint summary helper exists: `configured_endpoint_summary()` (re-exported from `observability.rs`; used in the debug banner at `crates/jackin/src/app.rs:242,306`) — sanitized authority rendering (no headers/credentials/key paths; contract: "Endpoint/TLS errors render sanitized authorities only").
- Host daemon protocol after plan 006: `DaemonRequest { id, protocol_version: 2, build_id, ctx, kind }`, dispatch at `host_daemon.rs:496-565`; `DaemonStatus { protocol_version, build_id, pid, socket_path, log_path, coredump_policy, adapters_enabled }` (`:75-84`) — `log_path` disappears with plan 013's daemon-log removal (leave the field until then; this plan adds health alongside).
- Capsule control protocol after plan 006: `ControlRequest{ctx, msg: ClientMsg}` — this plan adds a `TelemetryHealth` request/response variant pair.
- The E016 error path and `unsupported_otlp_protocol()` (`observability.rs:264`) provide the config-failure rendering pattern.
- Provider configuration is immutable for the process lifetime (contract) — validate never mutates a running process's providers; it validates ITS OWN process's config + delivery, and (optionally, via flags later — not this plan) queries daemon health.

## Target behavior: `jackin diagnostics validate`

```
$ jackin diagnostics validate
telemetry: endpoint grpc://collector.example:4317 (gzip, tls)
signals:   traces ok  logs ok  metrics ok
delivery:  confirmed (flush 128ms)
$ echo $?
0
```

- Emits one marked trace (`cli.command` root for the validate invocation itself — plan 007 already gives this — plus one dedicated child span def `telemetry.validate`), one marked log event (`telemetry.validate` event def, INFO, with a nonce field `telemetry.validate.nonce` — wait: no new attribute keys outside the closed registry! Use the existing bounded fields: the event name IS the marker; a nonce is unnecessary because delivery is confirmed by flush results, not backend query), and one marked metric increment (a `telemetry.validate` counter defined in the schema registry).
- Force-flushes all three providers; "confirmed" = every provider's flush returned success AND the outer export counters show ≥1 successful export attempt per signal after the flush (from `TelemetryHealth` — jackin❯-owned observation; the contract does not promise backend-ack semantics beyond flush/export success).
- Failure modes (each → non-zero exit + one-line reason + relevant E-code where applicable): no endpoint configured; `OTEL_SDK_DISABLED`; invalid config (protocol/E016, sampler conflict, malformed headers); flush timeout; export failure counted for any signal.
- Schema registry additions needed: `telemetry.validate` event + span + counter defs (add to `crates/jackin-telemetry/registry/`, regen constants — the extension registry table in the roadmap does not enumerate event NAMES, only attribute keys; event names live in the registry the same way `session.start` does).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Build | `cargo check -p jackin --all-targets --locked` | exit 0 |
| Tests | `cargo nextest run -p jackin -p jackin-diagnostics -p jackin-runtime -p jackin-capsule --all-features --locked` | all pass |
| Manual (no endpoint) | `cargo run --bin jackin -- diagnostics validate` | non-zero exit, "no endpoint configured" |
| Manual (bad protocol) | `OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4317 OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf cargo run --bin jackin -- diagnostics validate` | non-zero exit, E016 rendering |
| Lint | `cargo xtask ci --only lint` | exit 0 |

## Scope

**In scope:**
- `crates/jackin/src/cli/diagnostics.rs` (+ handler in `crates/jackin/src/app/` following the existing dispatch pattern in `cli/dispatch.rs`)
- `crates/jackin-diagnostics/src/observability/health.rs` (snapshot API surface for the command), a `validate_delivery()` orchestration fn in `jackin-diagnostics`
- `crates/jackin-telemetry/registry/` + regenerated schema (validate defs)
- `crates/jackin-runtime/src/host_daemon.rs` — `DaemonRequestKind::TelemetryHealth` + `DaemonResponseKind::TelemetryHealth(TelemetryHealthReport)`; sanitized config fingerprint (endpoint authority, gzip/tls flags, active signals, service identity — NO headers/paths/keys) as part of the report
- `crates/jackin-protocol/src/control.rs` + capsule daemon dispatch — `ClientMsg::TelemetryHealth` / `ServerMsg::TelemetryHealth{…}` variant pair (wire struct lives in jackin-protocol; populated from the capsule's health snapshot)
- `crates/jackin/src/app/daemon_cmd.rs` — `jackin daemon status` renders the daemon's telemetry health line from the new response (additive)
- Docs command page is plan 015's job; do not create docs here

**Out of scope:**
- Removing Summary/Compare (plan 013).
- Backend queries, saved queries, or any Parallax-side verification.
- Exposing SDK-internal counters (explicitly forbidden).

## Git workflow

- Branch: `feature/unified-otel-observability` — single branch, single PR for the whole roadmap item (plans 001–015 together); no per-plan branch or separate PR. Conventional Commits, e.g. `feat(cli): jackin diagnostics validate delivery check`. Sign `-s`, push after every commit.

## Steps

### Step 1: Schema defs + `validate_delivery()`

Add registry defs; implement `jackin_diagnostics::validate_delivery() -> Result<ValidationReport, ValidationFailure>`: check config (reuse `resolve_otlp_config` — plan 002), require enabled providers, emit the marked span/event/metric through the facade, force-flush all three providers with the 5 s budget, snapshot `TelemetryHealth` before/after, and compute per-signal delivery verdicts from the export counters. `ValidationFailure` variants map to the failure modes above.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features --locked -E 'test(validate)'` → pass (in-memory exporter: success path; disabled path; forced-flush-failure path via a failing test exporter).

### Step 2: CLI command

Add `Validate` to `DiagnosticsCommand` (clap doc: "Emit one marked log, trace, and metric and confirm OTLP delivery"), wire dispatch, render the report (operator output via the print-allowed CLI writer pattern used by sibling commands in `cli/diagnostics.rs` — the file already carries the `#![expect(clippy::print_stdout…)]` carve-out), exit non-zero on failure. Update the command-name mapping (plan 007 step 2) with `diagnostics.validate`.

**Verify**: manual smoke rows from the command table behave as specified.

### Step 3: Daemon health RPC (host daemon)

Add the request/response variants; handler builds `TelemetryHealthReport { fingerprint: SanitizedConfigFingerprint, health: TelemetryHealthSnapshot }` from the daemon process's own snapshot. The report types live in `crates/jackin-runtime/src/host_daemon.rs` beside the existing `DaemonStatus` (serde structs; the daemon protocol is runtime-owned, not jackin-protocol). Fingerprint fields: endpoint authority (host:port only), compression, tls on/off, sampler name, active signals, `service.name`, `app.mode` — nothing else.

**Verify**: daemon round-trip test (model on existing Hello/Status tests in `host_daemon` tests): request → response with populated snapshot; fingerprint contains no header values (assert serialized JSON lacks `authorization`, `header`, path-like strings for key material).

### Step 4: Capsule health RPC

Mirror on the control protocol: `ClientMsg::TelemetryHealth` → `ServerMsg::TelemetryHealth{report}` (wire struct in jackin-protocol; keep it serde-only). Capsule handler snapshots its process health incl. the `capsule_export` coverage-gap field (plan 010). Host `jackin daemon status` and (optionally) the capsule Debug-info dialog can render it later — wiring the host `daemon status` line is in scope; capsule TUI rendering is NOT (out of scope creep).

**Verify**: capsule control round-trip test passes; `cargo nextest run -p jackin-capsule -p jackin-protocol --locked` → pass.

## Test plan

- Unit: `validate_delivery` success/no-endpoint/disabled/flush-fail/export-fail paths (in-memory + failing exporters).
- CLI: exit codes + output shape (integration test in `crates/jackin/tests/` or the existing CLI test location — follow where `cli/diagnostics.rs` tests live today: sibling `cli/diagnostics/tests.rs`).
- RPC: both protocols round-trip typed health; privacy negative on the fingerprint.
- The real end-to-end validate-against-a-live-receiver test belongs to plan 014 (wire receiver) — add a `// covered further in plan 014` marker, not a duplicate here.

## Done criteria

- [ ] `cargo run --bin jackin -- diagnostics validate` exits non-zero with no endpoint, exits 0 against a working receiver (verified in plan 014's harness; here: exits non-zero with the four failure modes tested)
- [ ] `cargo nextest run --workspace --all-features --locked` exits 0
- [ ] Both daemons answer typed health over their protocols (tests)
- [ ] Fingerprint privacy negative passes
- [ ] `cargo xtask lint --strict` exits 0
- [ ] `plans/unified-otel-observability/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:
- Per-signal outer export counters (plan 002) cannot distinguish which signal failed at flush time — report the granularity actually available rather than faking per-signal verdicts.
- Adding control-protocol variants collides with in-flight plan 006/013 protocol edits (coordinate branch state first).
- Anything tempts you to add a new attribute key for the marker — the event NAME is the marker; the closed registry does not grow for this.

## Maintenance notes

- Plan 013 removes Summary/Compare and repoints the `commands/diagnostics.mdx` docs page (plan 015) at validate — keep this command's output stable; scripts will parse its exit code.
- `TelemetryHealthReport` is additive-only (daemon protocol version bumps otherwise).
- Reviewer focus: no SDK-internal counter names leak into the report; failure output renders sanitized authorities only.
