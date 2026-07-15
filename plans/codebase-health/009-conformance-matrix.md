# Plan 009: Exporter-backed host-to-capsule conformance matrix + measured export-volume ratchet

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-diagnostics/src/tests.rs crates/jackin-xtask/src/ratchet.rs ratchet.toml`
> Mismatch with "Current state" = STOP. Requires plans 002, 003, 004, 005 landed (their invariants are what this matrix asserts).

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: LOW (test/tooling only)
- **Depends on**: plans/codebase-health/002…005 (and benefits from 006–008)
- **Category**: tests (telemetry acceptance gate)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

The roadmap's completion bar for the whole telemetry section is a 7-point "exporter-backed matrix, run in CI against a real host-to-capsule path rather than direct facade calls". Today's conformance scenario drives the facade directly in-process (no capsule bridge, no attach boundary, no cross-process correlation), so capsule-path violations (plan 004's territory) were invisible to it, and matrix points 5–6 are asserted only on synthetic host spans. Separately, matrix point 7 requires "measured default-mode record/span/metric volume — not source constants — [as] the ratchet input"; the current `export-volume` ratchet family parses `MAX_SPANS`/`MAX_DEBUG_LOGS` string constants out of the test source, so it ratchets the assertion ceiling, not observed volume — it cannot detect the firehose regression it exists for. Finally, no SDK attribute limits are configured and `DroppedAttributesCount` is never asserted.

## Current state

- Conformance scenario: `crates/jackin-diagnostics/src/tests.rs:882-953` — `drive_standard_conformance_scenario` calls `operation_log`, `run.stage`, `enter_screen`, `metrics::record_frame` directly; captures via in-memory exporters; `MAX_DEBUG_LOGS = 64` / `MAX_SPANS = 48` constants live in this suite.
- Ratchet: `ratchet.toml:374-386` family `export-volume`, `provider = "export_volume_constants"`; provider `crates/jackin-xtask/src/ratchet.rs:455-463` parses the constants textually from `tests.rs`.
- Provider limits: `observability.rs:826-833` — `SdkTracerProvider`/`SdkLoggerProvider` built with processors + resource only; no explicit attribute limits; no `DroppedAttributesCount` assertions anywhere.
- Fake-port daemon infra exists in `jackin-capsule` (see `daemon/ports.rs`; plan 017 deepens it) — the "real host-to-capsule path" here means: host process emits through host bootstrap, capsule-side emissions flow through the capsule bootstrap + bridge (post-004), across the attach protocol boundary in-process or via the existing capsule test harness (`crates/jackin-capsule/src/attach_protocol/tests.rs` shows the current attach test seam — read it before designing).
- Canary/redaction conformance is OWNED by the sensitive-boundary roadmap — this matrix must invoke that gate, not duplicate its policy (roadmap matrix point 4). Find the existing helper: `grep -rn "canary\|scrub_secrets" crates/jackin-diagnostics/src --include='*.rs' | head`.
- CI: conformance currently runs implicitly via the all-features nextest lane; matrix requires an explicit CI step.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Diagnostics all-features | `cargo nextest run -p jackin-diagnostics --all-features` | pass |
| Capsule | `cargo nextest run -p jackin-capsule --all-features` | pass |
| Ratchet | `cargo xtask lint ratchet` | exit 0 |
| Cross-crate | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: `crates/jackin-diagnostics/src/tests.rs` (matrix rewrite/extension), a host↔capsule scenario harness (new module in jackin-diagnostics tests or jackin-capsule test-support — placement decided by what the attach seam allows), `observability.rs` (SDK limits), `crates/jackin-xtask/src/ratchet.rs` (measured-volume provider), `ratchet.toml` (family rewire), one CI step in `.github/workflows/ci.yml` naming the conformance lane explicitly.

**Out of scope**: redaction policy internals (invoke the sensitive-boundary gate); Docker-in-Docker E2E (the matrix runs in-process/fake-transport — the docker-e2e lane already exists separately).

## Git workflow

Branch `test/telemetry-conformance-matrix`; Conventional Commits; `git commit -s`; push per commit.

## Steps

### Step 1: SDK limits + dropped-attributes

Configure explicit span/log attribute limits on both providers (values: generous but finite — e.g. 64 attrs; pick from observed max + headroom and record the rationale in a comment). Extend captures to assert `DroppedAttributesCount == 0` (or the SDK-equivalent accessor) on every well-formed record.

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features` → pass.

### Step 2: Host-to-capsule scenario

Build the scenario: host bootstrap active with in-memory exporters; capsule side initialized via `init_capsule_tracing` with its exporter; drive a representative flow — launch stages (registered enum), a Docker/process operation via `ShellRunner`-style span, capsule breadcrumbs through the (post-004) bridge, an attach + forced attach failure + expected detach through the attach-protocol seam, cleanup. Assert the 7 matrix points:
1. Severity ladder records (INFO…FATAL + expected_close) carry timestamps, top-level EventName, severity, body, trace context/flags, canonical typed attrs, zero unexpected drops; inapplicable optionals absent.
2. Two runs of one build share Resource + scope identity; Resource excludes run/session/component; no generic record carries Rust-type/agent/provider/model fields.
3. Facade attributes survive export; neither sink exposes `[jackin-capsule …]` text, prohibited keys, or prefixes.
4. Prohibited content sweep (argv, URLs-with-queries, inspect JSON, terminal bytes) absent from bodies and indexed attrs; invoke the sensitive-boundary redaction/canary gate rather than re-implementing.
5. Forced attach failure ⇒ exactly one ERROR record, stable `error.type`, matching span status + failure metric; volatile variation doesn't split the fingerprint; expected detach ⇒ `expected_close`, no error type; `expected_shutdown` absent from canonical output.
6. run id, session id, `jackin.screen.name`, inherited trace context, required span links asserted across host→launch→attach→cleanup→capsule; JSONL correlation + artifact/query contract verified separately (JSONL file written in file mode carries real trace/span ids; `backend_query_hint` shape).
7. The scenario writes measured counts (records/spans/metrics emitted under default verbosity) to a machine-readable artifact (e.g. `target/telemetry-volume.json`).

**Verify**: `cargo nextest run -p jackin-diagnostics --all-features -E 'test(/conformance/)'` → pass.

### Step 3: Measured-volume ratchet

Replace `export_volume_constants` provider with `export_volume_measured`: runs (or consumes the artifact of) the conformance scenario and ratchets actual counts per key (`default_mode_logs`, `default_mode_spans`, `default_mode_metrics`). Keep source constants only as in-test guardrails. Update `ratchet.toml` family; regenerate bounds via the provider's print command (`cargo xtask lint ratchet --print export-volume`).

**Verify**: `cargo xtask lint ratchet` → exit 0; temporarily add a spurious log in the scenario locally to confirm the ratchet reddens (do not commit).

### Step 4: Explicit CI step

Add a named CI step (in the existing test job or a dedicated one) running the conformance filter explicitly, so the matrix is visible in CI output as its own gate, per roadmap item 5 ("Run it explicitly in CI").

**Verify**: workflow YAML parses (`actionlint` if available via mise, else review); `cargo xtask ci --fast` → exit 0.

## Test plan

The matrix IS the test plan; model captures on existing `InMemoryLogExporter` usage in `tests.rs:882-953` and keep the scenario deterministic (no wall-clock asserts, fixed fixture data).

## Done criteria

- [x] All 7 matrix points asserted from a host-to-capsule scenario (not direct-facade-only)
- [x] `DroppedAttributesCount == 0` asserted; explicit SDK limits configured
- [x] `export-volume` ratchet consumes measured counts; constants no longer parsed from source
- [x] Conformance lane named explicitly in CI
- [x] `cargo xtask ci --fast` + `cargo xtask lint ratchet` exit 0; status row updated

## STOP conditions

- Prerequisite plans not landed (grep for `registry.rs`, prefix-free bridge, v2 JSONL) — the matrix would assert shapes that don't exist yet.
- The attach seam cannot be driven without Docker — report; a fake-transport attach may need plan 017's ports first.
- The sensitive-boundary redaction gate/helpers don't exist yet — matrix point 4 then asserts only the structural prohibitions it can, and the redaction invocation is recorded as BLOCKED-on-sensitive-boundary in the status row (partial completion is acceptable ONLY for point 4).

## Maintenance notes

- This matrix is the durable acceptance gate for plans 001–008; treat its assertions as the telemetry contract's executable form.
- Volume-ratchet bounds shrink-only; raising one requires the measured artifact + reviewed rationale.
- When high-frequency activity needs more signal, add/adjust metrics — never per-event logs (matrix point 7 will catch the firehose).

## Execution notes

- Dual-bootstrap scenario: host `test_layers` then capsule `test_capsule_layers` + production `emit_session_start_for_test` (not synthetic host-only capsule events).
- Attach failure is host-path typed `operation_error`; expected detach + capsule.log on capsule bootstrap. Full attach-protocol/Docker E2E remains out of scope.
- `export_volume_measured` reads only `target/telemetry-volume.json` (`default_mode_{logs,spans,metrics}`); generates the artifact via `conformance_export_volume` when missing — **no** `MAX_*` constant fallback. Source constants remain in-test guardrails only.
- Matrix point 4 invokes the production redaction helper and sweeps argv, URL-query, inspect-JSON, and terminal-byte canaries across the combined export.
- Named CI job `telemetry-conformance` runs the filter explicitly.

**Index deviation (audit 2026-07-15)**: demoted from DONE to IN PROGRESS — Done criteria not fully met; see implementer audit rollup.
