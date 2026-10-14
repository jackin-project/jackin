# C06 — Capsule session control and prompt delivery

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Make programmatic control of an existing Capsule CLI session attributable, fenced and recoverable. Extend `jackin-protocol` and `jackin-capsule`; do not add tmux, another PTY runner or a reconstructed agent UI.

## 2. Existing foundation and gap

`SessionSend { session, text }` already sends literal bytes and appends no newline or bracketed-paste wrapper. `SessionSent` means bytes reached the PTY writer. This is useful transport, not a durable prompt contract. `Events` already provides a stream. Add workflow semantics while keeping existing manual behavior compatible. [J08](#evidence-and-references).

## 3. API and data contracts

```text
session.ensure(effect_id, start_spec, grant) -> SessionBinding
session.submit(command_id, session_address, prompt_ref, payload_digest,
               expected_lease_generation, expected_turn_id?) -> SubmissionReceipt
session.command_status(command_id) -> SubmissionReceipt
session.interrupt(command_id, session_address, expected_turn_id, lease) -> Receipt
session.inspect(session_address) -> SessionObservation
session.events(session_address, after_cursor) -> EventSubscription
session.capture(session_address, capture_policy) -> ArtifactReceipt
```

`SubmissionReceipt` distinguishes `recorded`, `bytes_written`, `agent_acknowledged`, `denied` and `delivery_unknown`, with receipt sequence, payload digest, physical session epoch and optional vendor message/turn ID. No status named simply `success` may blur those meanings. A session creation receipt is not “agent ready”; observation is separate.

## 4. Required functionality

Assign a new Capsule epoch on every supervisor incarnation and include it in session addresses and event cursors. Bind workflow identity to the actual session created through the existing spawn path. Before any automated write, validate the current host grant, input lease generation, target session epoch and expected turn where applicable.

Persist command intent in a bounded durable Capsule journal outside agent-writable paths. Deduplicate identical command IDs and payload digests. Return a prior receipt for a safe duplicate; reject a reused ID with different content. Serialize text and submit actions as one input transaction so operator input cannot interleave chunks. Cancellation between chunks records the partial-delivery boundary; it does not silently start over.

A PTY write cannot be atomically committed with the journal. After a crash during the critical write/receipt interval, mark the command unknown unless a certified vendor acknowledgement/lookup resolves it. Do not infer absence from silence or a visible prompt. Ask C17 to reconcile or require operator resolution. The user sees uncertainty rather than a fabricated completion.

Normalized events include process start/exit, transport receipt, vendor acknowledgement, turn active/blocked/completed, lease changes and capture references. Preserve source attribution: agent-reported status is an observation, not authority to accept code. Stream consumers can reconnect with a cursor; an unavailable history range returns an explicit gap and a current snapshot.

## 5. Legacy compatibility and security

Manual non-workflow sessions may continue using existing raw controls. A workflow-controlled session requires lease validation even for old raw `SessionSend` and attach input; old clients receive a clear unsupported/lease-required error, never a bypass. Child-session capabilities must not let an agent issue a host operator action or control a sibling session.

Journal payloads are encrypted/protected artifacts or references, with owner-only access. Log IDs and bounded result enums through telemetry, not prompt text. Limits cover queued command bytes, journal retention and event backpressure. When durable storage cannot record another input safely, reject automated input rather than continue without history.

## 6. Build instructions

Add versioned protocol messages and compatibility fixtures. Implement the Capsule journal/epoch primitives, then command deduplication and lease checks around the existing PTY writer. Add adapter input transactions and structured-transport mediation to that same authority. Finally add resumable event subscriptions/capture. Test fault injection before enabling production autonomous input.

## 7. Dependencies and integration

C05 translates vendor behavior; C07 owns leases; C10 records host intent; C17 reconciles ambiguity; C22 stores replay. The existing runtime spawn/attach functions remain the lifecycle path.

## 8. Acceptance tests

`CAP-01`: duplicate command ID writes once when receipt is known. `CAP-02`: same ID/different digest is rejected. `CAP-03`: kill before/inside/after PTY write yields correct known/unknown state and never blind resend. `CAP-04`: stale epochs cannot route to a reused numeric session ID. `CAP-05`: long input is not interleaved or truncated. `CAP-06`: journal-full failure stops input. `CAP-07`: all acknowledgements preserve the difference between transport, vendor turn and verification.

## Evidence and references

- **J05 — [Runtime public integration surface](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime/src/lib.rs).** Full file. Existing launch, inspect, spawn, attach and cleanup re-exports; reactive daemon feature boundary.
- **J07 — [Shared Capsule contracts](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-protocol/src/lib.rs).** Lines 1–280. Session capability and identity environment, scoped credential contracts, CapsuleConfig and exit preservation choices.
- **J08 — [Capsule control protocol](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-protocol/src/control.rs).** Lines 1–280. SessionSend writes literal bytes; SessionSent acknowledges PTY writer delivery; Events is a subscription. Not an exactly-once prompt protocol.
- **J09 — [Capsule binary attach protocol](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-protocol/src/attach.rs).** Lines 1–240. Attach framing, raw input/output, resize, host-action tags, limits and spawn request. Full input routing implementation not audited.
