# Plan 006: Cross-process context — versioned W3C envelopes on every protocol, CLIENT/SERVER RPC spans

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- crates/jackin-protocol crates/jackin-runtime/src/host_daemon.rs crates/jackin-runtime/src/exec_host.rs crates/jackin-capsule/src/socket.rs crates/jackin-capsule/src/client.rs crates/jackin-runtime/src/runtime/host_attach.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: HIGH (wire-format changes; host and capsule binaries must move together — pre-release breaking changes are allowed, no migration shims: see `PRERELEASE.md`)
- **Depends on**: plans/unified-otel-observability/004-telemetry-facade-api.md, 005-async-spawn-helpers.md
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements the wire half of "Async and cross-process context" and the "Capsule control RPC" / "Host daemon RPC" / "Attach protocol" rows of the case contract; the roadmap item is the binding contract and overrides this plan on any conflict.
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

The contract: W3C `traceparent`/`tracestate` are the only trace propagation format; Baggage is disabled; attach, control, daemon, and detached-job envelopes carry a **versioned string map** plus only the required invocation/session/job correlation fields; the caller creates the `CLIENT`/`PRODUCER` span, injects, sends; the receiver extracts BEFORE constructing the `SERVER`/`CONSUMER` span; malformed W3C data is ignored without echo and starts a local root; malformed product correlation IDs reject the request; a valid remote unsampled decision is honored. Today NO wire protocol carries any correlation field — cross-process context is env-only at container launch (`TRACEPARENT` env). Synchronous CLI→daemon and host→capsule-control calls therefore cannot form one trace. This plan adds the envelope and the RPC span shapes.

## Current state

(verified at planning commit)

- `crates/jackin-protocol/src/control.rs:15-76` — `ClientMsg` (length-prefixed JSON, `#[serde(tag = "type", rename_all = "snake_case")]`): `Status`, `Snapshot`, `Agents`, `ReportRuntimeEvent{session_id, source_id, runtime, event, payload}`, `StatusCapture{session_id}`, `UsageFocused`, `UsageRefreshFocused`, `UsageAccountList`, `ExecCommand{command, args}`, `TokenUsage{session_id}`, `#[serde(other)] Unknown`. `ServerMsg` at `:100` (SessionList/Snapshot/AgentRegistry/Ack/UsageFocused/UsageAccounts/ExecResult/ExecDenied/TokenUsage/Unknown). **No version field, no request id, no context field.** Framing: `frame` at `:639` (BE u32 length + JSON). `session_id: u64` here is a pane/leaf index, NOT the telemetry session id.
- `crates/jackin-protocol/src/attach.rs` — binary frames `[tag][BE u32 len][payload]`; `ClientFrame::Hello { rows, cols, spawn, env, focus_session, terminal }` (`:689-705`, tag 0x01, `MAX_HELLO_ENV=64` at `:119`); `ServerFrame` at `:751`. Control-vs-attach channel disambiguated by leading byte (`:22-27`). No context field anywhere.
- `crates/jackin-runtime/src/host_daemon.rs:26-45` — `DaemonRequest { id: String, protocol_version: u16, build_id: String, #[serde(flatten)] kind }`; `DaemonRequestKind::{Hello, Status, AttentionSnapshot{…}, Shutdown}`; newline-delimited JSON over `~/.jackin/run/jackin-daemon.sock`; `DAEMON_PROTOCOL_VERSION: u16 = 1` (`:20`). Server loop: `serve` `:335` → `handle_stream` `:450` → `handle_request_line` `:480` → `handle_request` `:496-565`. Client: `request()` at `:370` (sends `id: "cli"`).
- `crates/jackin-runtime/src/exec_host.rs` — host.sock credential resolver: accept loop `:134`, `handle_connection` `:149`, peer auth `SO_PEERCRED` `:216-251`. Capsule client side: `crates/jackin-capsule/src/exec.rs:147,291`.
- Capsule sockets: bind `crates/jackin-capsule/src/socket.rs:114`, accept `:138`, `read_control_msg` `:224`, `write_control_reply` `:275`; handshake `crates/jackin-capsule/src/daemon.rs:1122`; host-side control/attach clients `crates/jackin-capsule/src/client.rs:62,527`, `crates/jackin-runtime/src/runtime/host_attach.rs:115`, probes `runtime/attach.rs:107,199`, `runtime/snapshot.rs:145`.
- Existing env-based propagation (stays for process launch): `launch_runtime.rs:750-760` injects `OTEL_EXPORTER_OTLP_ENDPOINT` + `TRACEPARENT`; capsule reads them in `jackin-usage/src/telemetry.rs:47-48`; `parse_traceparent` at `jackin-diagnostics/src/observability.rs:1157`; `current_traceparent()`/`format_traceparent` at `jackin-diagnostics/src/screen.rs:341,377`.
- Roadmap RPC method registries (closed sets):
  - Capsule control: `rpc.system.name=jackin`; `rpc.method` ∈ `jackin.capsule.Control/Status | Snapshot | Agents | ReportRuntimeEvent | StatusCapture | UsageFocused | UsageRefreshFocused | UsageAccountList | ExecCommand | TokenUsage` (maps 1:1 to `ClientMsg` variants).
  - Host daemon: `jackin.host.Daemon/Hello | Status | AttentionSnapshot | Shutdown`.
  - Attach protocol: only bounded handshake/detach/focus/clipboard-image-transfer control operations get spans; resize/input/output/PTY frame streams are metrics/state events, never per-frame spans.
- `PRERELEASE.md`: breaking wire changes allowed, no compatibility shims; host+capsule ship together (`crates/jackin-protocol/AGENTS.md`: "A wire-format change is a host↔capsule contract change: align both binaries in the same PR").

## Target envelope

One shared type in `crates/jackin-protocol` (new module `src/telemetry_context.rs`):

```rust
/// Versioned cross-process correlation envelope. v1.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryContext {
    /// Envelope version. Bump on layout change. v1 = this struct.
    pub v: u16,                                  // = 1
    /// W3C trace context. Absent/malformed => receiver starts a local root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traceparent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracestate: Option<String>,
    /// Product correlation (validated; malformed => reject request).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation_id: Option<String>,           // cli.invocation.id (UUID)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,              // session.id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,                  // job.id (detached jobs only)
}
```

No Baggage. No other keys ("a versioned string map plus only the required invocation/session/job correlation fields"). Carriage per protocol:
- **Control (capsule)**: wrap the existing enum — new outer struct `ControlRequest { ctx: TelemetryContext, msg: ClientMsg }` (and mirror `ControlResponse` without ctx; responses don't need context). Pre-release break; both binaries updated together.
- **Attach**: add `context: Option<TelemetryContext>` field to `ClientFrame::Hello` (bincode-style manual codec — extend the encoder at `attach.rs:787` region and decoder at `:1278-1358`; bump the handshake by requiring the field in the new build — old/new mixing is not supported pre-release).
- **Host daemon**: add `ctx: TelemetryContext` field to `DaemonRequest` and bump `DAEMON_PROTOCOL_VERSION` to 2 (version mismatch already rejects at `:503-520`).
- **exec host.sock**: add `ctx` to the request struct used by `exec_host.rs`/`capsule/src/exec.rs` (find the shared type: `grep -rn "CredRequest" crates/jackin-protocol/src/lib.rs crates/jackin-capsule/src/exec.rs crates/jackin-runtime/src/exec_host.rs`).
- **Detached prewarm jobs**: in-process today (no wire) — the envelope type is still used as the job record carried into the spawned task (plan 010 consumes it).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Protocol tests | `cargo nextest run -p jackin-protocol --locked` | all pass |
| Workspace | `cargo nextest run --workspace --all-features --locked` | all pass |
| Fuzz targets still build | `cargo check -p jackin-protocol --all-targets --locked` (fuzz crate: `ls crates/jackin-protocol/fuzz 2>/dev/null || true` — update targets if present; protocol is fuzzed in CI per `ci.yml` fuzz job) | exit 0 |
| Capsule+console snapshots | `cargo nextest run -p jackin-capsule -p jackin-console --locked` | all pass |
| Lint | `cargo xtask ci --only lint` | exit 0 |

## Scope

**In scope:**
- `crates/jackin-protocol/src/telemetry_context.rs` (new, + `telemetry_context/tests.rs`), `src/control.rs`, `src/attach.rs`, `src/lib.rs`
- `crates/jackin-telemetry/src/propagation.rs` (new): `inject(ctx: &mut TelemetryContext)` from current span, `extract(&TelemetryContext) -> ExtractOutcome { Parent(SpanContext) | LocalRoot | RejectRequest }` — wraps W3C parse (reuse/port `parse_traceparent`/`format_traceparent` logic from `jackin-diagnostics`), validates product IDs (UUID shape for invocation/job; `jk-`-prefixed or opaque non-empty ≤ 64 for session), honors remote sampled flag. NOTE: `jackin-protocol` is T1 and `jackin-telemetry` is T0, so protocol may depend on telemetry if needed — but keep `TelemetryContext` serde-only in protocol and the OTel logic in jackin-telemetry to respect "data contracts only" (`crates/jackin-protocol/AGENTS.md`).
- Send/receive sites: `crates/jackin-runtime/src/host_daemon.rs` (server + `request()` client), `crates/jackin-runtime/src/exec_host.rs`, `crates/jackin-runtime/src/runtime/host_attach.rs` (+ `runtime/attach.rs`, `runtime/snapshot.rs` probes), `crates/jackin-capsule/src/socket.rs`, `src/client.rs`, `src/daemon.rs` (handshake + control dispatch), `src/exec.rs`, `src/tui/run.rs`
- CLIENT/SERVER span creation at those sites using plan 004's operation API with schema span defs (`rpc.method` registry above; Unix transport attrs; **no request payload fields** — container/pane data, exec argv, and usage payloads are prohibited)

**Out of scope:**
- The launch-time env injection (`TRACEPARENT`/endpoint env at `launch_runtime.rs:750-766`) — stays as the process-boundary carrier; plan 007 re-points what it carries.
- Session/invocation id minting semantics (plan 007). This plan threads fields; 007 fills them.
- Attach input/output/resize stream instrumentation (plan 010 metrics).
- The `reactive_daemon.rs` spike (feature-gated, "intentionally not a production daemon") — leave untouched.

## Git workflow

- Branch: `feature/unified-otel-observability` — single branch, single PR for the whole roadmap item (plans 001–015 together); no per-plan branch or separate PR. Conventional Commits, e.g. `feat(protocol)!: versioned telemetry context on control, attach, and daemon envelopes` (breaking-change marker per Conventional Commits; add `BREAKING CHANGE:` footer noting host+capsule must be rebuilt together — trivially satisfied since both ship in the same PR). Sign `-s`, push after every commit.

## Steps

### Step 1: Envelope type + propagation module

Add `TelemetryContext` (protocol) and `propagation.rs` (jackin-telemetry) per the shapes above. Extraction rules to encode exactly: malformed `traceparent` → `LocalRoot` (never echo the bad value into any log body — log only `outcome=failure, reason=malformed` shaped event later); malformed product id → `RejectRequest`; valid unsampled parent → parent applied with sampled=false (the `ParentBased(AlwaysOn)` sampler from plan 002 then honors it).

**Verify**: `cargo nextest run -p jackin-protocol -p jackin-telemetry --locked` → new unit tests pass (round-trip serde, malformed-W3C → LocalRoot, bad-UUID → RejectRequest, unsampled honored).

### Step 2: Host daemon RPC (both ends)

Client (`host_daemon.rs:370` `request()`): create CLIENT span (`rpc.method="jackin.host.Daemon/<Kind>"`, `rpc.system.name=jackin`) via the facade, `inject`, send `DaemonRequest{ctx, …}`, complete guard with outcome from the response. Server (`handle_request_line`/`handle_request`): parse, `extract`, then construct the SERVER span with the extracted parent BEFORE dispatching the match at `:522-564`; `RejectRequest` → respond `Error{message:"invalid correlation"}` without processing. Bump `DAEMON_PROTOCOL_VERSION` to 2. The daemon is a **persistent process**: apply request correlation only within the request scope — never store the extracted context beyond the response (roadmap: "A persistent daemon applies request correlation inside that request and does not retain stale invocation/session context for autonomous cycles").

**Verify**: `cargo nextest run -p jackin-runtime --locked -E 'test(daemon)'` → pass, including a new same-trace test (client TraceId == server span TraceId via in-memory export).

### Step 3: Capsule control RPC (both ends)

Wrap `ClientMsg` in `ControlRequest{ctx, msg}`; update `frame`/read paths (`control.rs:639`, `socket.rs:224,275`) and every sender (`client.rs`, `snapshot.rs`, exec paths, usage CLI — find all: `grep -rn "ClientMsg::" crates/ --include='*.rs' | grep -v tests`). CLIENT span per send with `rpc.method="jackin.capsule.Control/<Variant>"`; SERVER span around dispatch in the capsule daemon. `ExecCommand` spans carry executable classification only — never `command`/`args` values (prohibited attributes; the roadmap's operator-approved `jackin-exec` row).

**Verify**: `cargo nextest run -p jackin-capsule -p jackin-runtime --locked` → pass; new round-trip test asserting parentage across the framed boundary (serialize → deserialize → extract).

### Step 4: Attach handshake + exec host.sock

Add `context` to `ClientFrame::Hello` codec (encode at `attach.rs:867` region, decode at `:1278-1358`; respect `MAX_HELLO_ENV` unrelatedly). Host side injects in `run_attach_protocol` (`host_attach.rs:238` Hello construction); capsule handshake (`daemon.rs:1122` `perform_handshake`) extracts and parents its bounded handshake span. Same for the exec host.sock request/response pair. Only bounded control operations get spans (handshake, detach, focus change ack, clipboard-image transfer) — the output/input pump loops get NOTHING here.

**Verify**: `cargo nextest run -p jackin-protocol -p jackin-capsule --locked` → codec tests updated + passing (attach.rs has extensive frame tests — extend them for the new field).

### Step 5: Propagation conformance test

New tests in the telemetry conformance group are selected by CI with `-E 'test(/conformance/)'` over `-p jackin-diagnostics -p jackin-capsule -p jackin-runtime`: full matrix — {valid parent, malformed traceparent, missing ctx, unsampled parent, bad invocation id} × {daemon RPC, control RPC} asserting {same trace, local root, local root, unsampled honored, rejected}.

**Verify**: `cargo nextest run -p jackin-diagnostics -p jackin-capsule -p jackin-runtime --all-features --locked -E 'test(/conformance/)'` → all pass.

## Reopened audit additions (2026-07-16)

- CLIENT/SERVER ownership ends only after response serialization and the actual socket write; response-write failures receive bounded outcomes and `error.type`.
- Attach handshake, detach, focus, and clipboard-image-transfer controls are bounded operations. Streams remain metric/state only and never receive lifetime spans.
- Emit RPC count, active, and duration metrics keyed only by registered `rpc.method`, outcome, and stable error type.
- The serialized `{valid, malformed, missing, unsampled, bad product id} × {host daemon, Capsule control}` matrix proves same-trace parentage, local-root fallback, sampling preservation, rejection, and no side effects.
- The `jackin-exec` host credential socket carries the shared envelope and uses CLIENT/SERVER operations through the actual reply write. Invalid product correlation is extracted and rejected before constructing a local SERVER owner; that owner emits one typed `rpc_error`, returns the bounded protocol error, consumes reply-write failure, and completes as failure without allowing the listener to emit a duplicate terminal error. Focused exporter tests prove the bad-ID root/error count and the peer-close write-failure path.
- Before an exec request can be decoded, peer-authentication failure, truncated length, oversized length, truncated body, and malformed JSON are consumed at the fixed credential-resolver RPC boundary. Each creates exactly one local bounded SERVER failure only after decoding has failed, emits exactly one typed `rpc_error`, never formats the peer error or payload, and returns cleanly so the accept loop cannot double-report it. A four-shape exporter matrix proves one root and one error per framing failure.

## Test plan

- Unit: envelope serde round-trip (JSON for control/daemon; binary codec for attach), extraction matrix, id validation boundaries (empty, >64 chars, non-UUID invocation id).
- Integration: same-trace CLIENT/SERVER tests per protocol (steps 2–3), reject path returns protocol error without side effects.
- Conformance: step 5 matrix.
- Model on existing protocol tests (`crates/jackin-protocol/src/attach/tests.rs` frame round-trips) and daemon tests in `jackin-runtime`.

## Done criteria

- [ ] `cargo nextest run --workspace --all-features --locked` exits 0
- [ ] `grep -rn "TelemetryContext" crates/jackin-protocol/src/ crates/jackin-runtime/src/host_daemon.rs crates/jackin-capsule/src/socket.rs` shows the envelope on all three protocols
- [ ] Conformance propagation matrix passes
- [ ] `grep -rn "baggage" crates/ --include='*.rs' -i | grep -v tests` returns no matches (Baggage stays disabled)
- [ ] No request-payload attribute on any RPC span (spot-check `ExecCommand` span test asserts absence of `command`/`args`)
- [ ] `plans/unified-otel-observability/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:
- The attach binary codec cannot grow an optional field without breaking the existing tag/length framing invariants (channel disambiguation at `attach.rs:22-27`).
- Any consumer of these protocols exists outside the workspace (search docs for third-party protocol clients) — the pre-release breaking assumption would be wrong.
- You find a second sender of `DaemonRequestKind::AttentionSnapshot` beyond the spike (`reactive_daemon.rs:256,263`) — the plan assumes production senders are Hello/Status/Shutdown only.
- Honoring remote unsampled decisions conflicts with the sampler built in plan 002 (would indicate 002 drifted).

## Maintenance notes

- The envelope is v1; any layout change bumps `v` and both binaries together (pre-release: no shim).
- Plan 007 fills `invocation_id`/`session_id` with real semantics; plan 010 fills `job_id`. Until then fields flow as `None` — that is expected.
- Reviewer focus: extract-before-span-construction ordering on every server path, and no stored context in the persistent daemon.
