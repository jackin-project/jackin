# API, prompt delivery and operator-resolution examples

## Status

All identifiers/hashes below are fictional fixtures. These are proposed workflow-service payloads inside the negotiated transport, not currently accepted Jackin daemon messages. Every mutation is authenticated and version-checked; artifact and lease access is separately authorized.

## Plan and start

```json
{
  "api_version": "workflow.v1",
  "request_id": "request-plan-001",
  "method": "definitions.plan",
  "idempotency_key": "plan-example-001",
  "params": {
    "source_bundle_id": "artifact-source-001",
    "schema_version": 1,
    "parameters": {"task_id": "verified-rust-change"}
  }
}
```

```json
{
  "api_version": "workflow.v1",
  "request_id": "request-start-001",
  "method": "runs.start",
  "idempotency_key": "start-example-001",
  "params": {
    "plan_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  }
}
```

```json
{
  "api_version": "workflow.v1",
  "request_id": "request-start-001",
  "result": {
    "run_id": "run-example-001",
    "operation_id": "operation-start-001",
    "state": "planned",
    "state_version": 1,
    "event_cursor": "1"
  }
}
```

This reply means durable start acceptance, not completed implementation. Calling again with the same idempotency key and identical payload returns the same run. A different plan under the same key returns conflict.

## Internal Capsule submission

This is a host-to-Capsule operation, not a public permission for any client to type into any session.

```json
{
  "command_id": "command-feedback-002",
  "session_address": {
    "instance_id": "instance-implement-001",
    "capsule_epoch": "capsule-epoch-003",
    "capsule_session_id": 7
  },
  "logical_session_id": "session-implement-001",
  "prompt_artifact_id": "artifact-feedback-002",
  "payload_digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "expected_lease_generation": 8,
  "expected_turn_id": "vendor-turn-001"
}
```

```json
{
  "command_id": "command-feedback-002",
  "receipt_seq": "2",
  "state": "bytes_written",
  "bytes": 1642,
  "agent_acknowledged": false,
  "capsule_epoch": "capsule-epoch-003"
}
```

The receipt does not say the agent processed the prompt, finished a turn, repaired code or passed verification. Those are later distinct records. Bytes include the exact adapter-managed input transaction, whose framing/submit actions are recorded separately from user-authored literal content.

## Crash ambiguity

After a crash in the write/receipt window, the host may receive:

```json
{
  "command_id": "command-feedback-002",
  "state": "delivery_unknown",
  "code": "DELIVERY_UNKNOWN",
  "automatic_resend_allowed": false,
  "required_action": "reconcile_or_operator_resolution"
}
```

The run enters a safe reconciliation/operator state. A missing terminal line is not proof of non-delivery. The host queries the durable Capsule journal and any certified vendor message record. If proof is unavailable, an authorized operator resolves the uncertainty explicitly:

```json
{
  "api_version": "workflow.v1",
  "request_id": "request-resolve-001",
  "method": "commands.resolve_delivery",
  "idempotency_key": "resolve-input-example-001",
  "params": {
    "run_id": "run-example-001",
    "command_id": "command-feedback-002",
    "expected_run_version": 23,
    "resolution": "authorize_replacement",
    "reason_artifact_id": "artifact-operator-reason-001"
  }
}
```

Allowed operator resolutions are `observed_received`, `authorize_replacement` and `abandon`. An operator attestation is recorded as such, not as a vendor acknowledgement. An authorized replacement uses a **new command ID**, links `replacement_of` to the unknown original, reacquires a valid input lease and warns that the original may have been processed. That is an explicit decision, not a silent retry. Original journal history is never overwritten.

## Exclusive control

```json
{
  "api_version": "workflow.v1",
  "request_id": "request-control-001",
  "method": "sessions.acquire_control",
  "idempotency_key": "control-example-001",
  "params": {
    "run_id": "run-example-001",
    "logical_session_id": "session-implement-001",
    "expected_generation": 8,
    "ttl_ms": 30000,
    "reason_artifact_id": "artifact-operator-reason-002"
  }
}
```

The service first fences scheduler input and serializes any in-flight input transaction. Only then does it issue generation 9. Lease renewal does not change the session target. Old-generation input is rejected. Observe never acquires this lease.

## Exact-revision approval

```json
{
  "api_version": "workflow.v1",
  "request_id": "request-approve-001",
  "method": "gates.approve",
  "idempotency_key": "approve-example-001",
  "params": {
    "run_id": "run-example-001",
    "approval_request_id": "approval-publication-001",
    "checkpoint_id": "checkpoint-example-002",
    "expected_candidate_ordinal": 2,
    "expected_run_version": 41,
    "reason_artifact_id": "artifact-operator-reason-003"
  }
}
```

If candidate ordinal 3 or new unsealed mutations supersede the finalization boundary, this request fails with `STALE_REVISION`/`STATE_CONFLICT`. The host must not approve whichever revision is now current. The operator refreshes exact evidence and makes a new decision.
