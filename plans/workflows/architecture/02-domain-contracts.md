# Canonical domain, data and wire contracts

**Status:** Proposed v1 contract. This file is authoritative for common vocabulary used by every component and block. Rust snippets are interface sketches, not claims of compiled implementation.

## Identity and version rules

Use opaque host-generated IDs for runs, commands, attempts, artifacts, checkpoints, findings, decisions and effects. Human `node_id` and workflow names are stable ASCII identifiers matching `[a-z][a-z0-9_-]{0,63}`; display labels are separate untrusted text. IDs never authorize access by themselves.

A physical session address is `(instance_id, capsule_epoch, capsule_session_id)`. Store the vendor conversation/thread ID separately and relate it to a durable `logical_session_id`. A numeric Capsule session ID may be reused after restart; the epoch prevents misrouting. `attempt_key` is `(run_id, node_id, iteration, attempt_number)`. Feedback continuation does not create a new logical implementer session.

`schema_version: 1` identifies the domain/wire major version. Additive optional fields require compatibility tests; unknown enum values are rejected at execution boundaries, not interpreted as success. Version the plan/compiler/evaluator and event schemas independently. The existing host-daemon protocol version is not the workflow API version.

Hashes use `sha256:<64 lowercase hex digits>`. Git identity is `{algorithm: "sha1"|"sha256", oid: "<validated hex>"}` with length checked against the algorithm. Never truncate IDs or Git object IDs for authority checks. Timestamps are UTC RFC 3339. Durations use integer milliseconds, CPU uses integer millicores and storage/memory use integer bytes. JSON numeric fields are bounded to exact JavaScript-safe integers; larger counters such as event sequence numbers are decimal strings. Floating-point NaN, infinity and implicit unit strings are invalid in the canonical plan.

## Immutable plan

```text
ExecutionPlanV1 {
  schema_version, compiler_build, pklr_artifact_digest,
  workflow_id, source_bundle_digest, import_lock_digest,
  resolved_parameters, block_catalog_version,
  nodes: [CompiledNode], dependency_edges: [TypedEdge],
  repair_scopes: [RepairScope], repository_inputs: [RepositoryInput],
  role_inputs: [RoleInput], attachment_inputs: [ArtifactRef],
  runtime_requirements: [CertifiedRuntimeRequirement],
  authorization_ceiling: AuthorizationCeiling,
  budget: RunBudget, retention: RetentionPolicy,
  acceptance_policy: AcceptancePolicy
}
PlanEnvelope { plan_hash, canonical_bytes_digest, plan }
```

The hash covers canonical plan bytes and a domain-separation prefix. It excludes its own hash, mutable execution state, wall-clock compilation timestamps and secret values. It includes all semantically effective defaults. Preserve the original source bundle separately for reproduction and editing. A policy/grant change that alters the ceiling produces a new plan; runtime revocation may narrow or stop an existing run but never silently widen its frozen ceiling.

`RoleInput` includes role identity, exact source revision/manifest digest, image digest and declared startup/setup contract digest. Runtime binaries mounted independently of the image carry their own digests. `RepositoryInput` includes repository identity, source locator, exact starting Git OID, isolation mode, submodule/LFS policy and mount destination. `CertifiedRuntimeRequirement` records agent, provider/model selection, binary/protocol identity and required capabilities; it does not contain credentials.

## Common node contract

```text
CompiledNode {
  id, kind, label,
  inputs: map<port, InputBinding>, outputs: map<port, OutputContract>,
  depends_on: [NodeId], when: optional<TypedExpression>,
  role_ref: optional<RoleInputId>, sandbox_ref: optional<GrantTemplateId>,
  limits: NodeLimits, retry: RetryPolicy,
  completion: CompletionContract, config: KindSpecificConfig
}
InputBinding = Literal | Artifact | Repository | NodeOutput | IterationInput
OutputContract { type_id, schema_digest, required, max_bytes }
```

All nodes declare limits, permissions and completion, including host-local condition/join nodes. Defaults from typed templates are allowed but must become explicit in the compiled plan. A role is required only for blocks that actually use an execution environment. Host-local pure blocks receive no implicit shell, network or filesystem capability.

The port type registry includes `text/v1`, `json/v1`, `context-bundle/v1`, `repository-ref/v1`, `checkpoint/v1`, `test-result/v1`, `review-result/v1`, `finding-set/v1`, `decision/v1`, `acceptance-certificate/v1`, `artifact-set/v1`, `publication/v1`, `session-ref/v1`, `agent-turn-result/v1`, `command-result/v1`, `parallel-group/v1`, `join-result/v1`, `submission-receipt/v1` and `git-conflict/v1`. A JSON port must name a pinned schema and size limit; “any JSON” is not an acceptance contract. Compiler-inserted adapters are prohibited unless explicit in the plan.

## Runtime records

| Record | Required fields beyond its ID |
|---|---|
| Run | plan_hash, state, state_version, controller_epoch, created_at, budget_counters, cancellation_reason, terminal_disposition |
| NodeAttempt | run/node/iteration/attempt identity, state, state_version, inputs_digest, grant_digest, instance/session refs, started_at, deadline_at, result_ref, failure_class |
| SessionBinding | logical session, physical address, vendor ID, adapter certificate, current_turn_id, lease_generation, reconciliation_state |
| Command | command_id, target physical/logical session, payload artifact/digest, expected turn, expected lease generation, delivery state, receipt refs |
| Checkpoint | repository ID, producer attempt, ordinal, base/head/tree OIDs, source_manifest_digest, capture method, barrier proof, sealed bundle/artifact refs |
| Verification | target checkpoint, verifier attempt, harness/policy digest, declared command digest, result, evidence refs, started/finished times |
| Decision | actor identity, action, target checkpoint/plan, policy version, expected candidate ordinal, reason artifact, expiry, decision state |
| Publication | effect ID, action, repository/remote, target OID, expected old ref, authorization decision, remote receipt, reconciliation state |
| PreservationHold | resource/artifact/checkpoint identity, reason, protected descendants, copy evidence, release authority, created/expiry policy |

## Findings and acceptance

```json
{
  "schema_version": 1,
  "finding_id": "finding-example-001",
  "checkpoint_id": "checkpoint-example-002",
  "severity": "high",
  "category": "correctness",
  "summary": "Cancellation is not propagated to the child process.",
  "location": {"repository_path": "src/worker.rs", "start_line": 41, "end_line": 55},
  "evidence_refs": ["artifact-example-004"],
  "suggested_action": "Terminate the process group and test descendant cleanup.",
  "disposition": "open"
}
```

Locations are optional when a finding concerns behavior rather than one source span. Lines are validated against the target checkpoint; paths cannot escape it. Findings may be superseded, addressed, dismissed by authorized review, or waived under an explicit policy. An agent marking its own finding “fixed” is a claim; independent verification supplies the closing evidence.

`AcceptanceCertificate` binds plan hash, final candidate ordinal/checkpoint/OIDs, policy and harness digests, required verification IDs, review findings/dispositions, waiver IDs, host decision version and allowed publication actions. It is a host-issued record, not an agent-produced JSON file. Host authenticity is required locally; export signatures are separately versioned. It is invalid for a different revision or revoked policy.

## Event envelope

```json
{
  "schema_version": 1,
  "run_id": "run-example-001",
  "seq": "57",
  "event_id": "event-example-057",
  "type": "checkpoint.sealed",
  "occurred_at": "2026-10-14T09:00:00Z",
  "actor": {"kind": "host", "id": "workflow-controller"},
  "node_id": "implement",
  "attempt_id": "attempt-example-001",
  "correlation_id": "command-example-003",
  "payload": {"checkpoint_id": "checkpoint-example-002", "ordinal": 2}
}
```

Sequence order is authoritative per run. Cross-run timestamps do not define causal order. Event delivery is at least once; consumers deduplicate `(run_id, seq)`. Persisted events must not include plaintext secrets. Larger prompts, findings and terminal records are protected artifact references, not general telemetry attributes.

## Error contract

`WorkflowError { code, message, retry_class, details, correlation_id }`. Messages are sanitized; details use bounded typed fields rather than arbitrary command output.

| Code family | Required meaning / treatment |
|---|---|
| `CONFIG_INVALID`, `GRAPH_INVALID`, `PORT_MISMATCH` | Validation fails before worker launch; include source span and corrective diagnostic. |
| `POLICY_DENIED`, `CAPABILITY_UNSUPPORTED`, `CERTIFICATE_MISSING` | Fail closed. Retry cannot bypass the missing authority or capability. |
| `STATE_CONFLICT`, `STALE_REVISION`, `LEASE_CONFLICT`, `LEASE_EXPIRED` | Re-read state; do not blindly repeat the mutation. |
| `DELIVERY_UNKNOWN`, `EFFECT_UNKNOWN` | Reconciliation/operator action required unless safe lookup/dedup proves the result. |
| `RESOURCE_EXHAUSTED`, `DEADLINE_EXCEEDED` | Stop dispatch, preserve work and evaluate configured disposition. |
| `VERIFICATION_FAILED`, `REVIEW_INVALID` | Evidence outcome; eligible for bounded repair only as policy allows. |
| `STORAGE_UNAVAILABLE`, `INTEGRITY_FAILURE` | Stop authoritative mutations/effects. Never report success from transient memory. |
| `BACKEND_UNAVAILABLE`, `AGENT_UNAVAILABLE` | Bounded infrastructure retry only when the prior effect is proven absent/reconciled. |

## Side-effect classification

Pure evaluation and graph reduction are replayable. Instance creation, prompt submission, Git mutation, upload, PR creation and cleanup are effectful. Each effect declares an idempotency key, a reconciliation lookup and an ambiguity policy. A “retryable” network error does not make the preceding remote mutation safe to repeat.

## Finalization and explicit uncertainty resolution

`FinalizationRecord` binds run/scope, candidate ordinal/checkpoint, exact tree/source manifest, write-barrier proof, writer-fence generation, policy version and active/revoked state. The host confirms that no newer or unsealed mutation supersedes the candidate before final acceptance. Acceptance certificates additionally reference this record. Reopening mutation revokes finalization/acceptance eligibility before the write barrier is released.

`DeliveryResolution` binds original command, authorized actor, expected run/session version, one of `observed_received`, `authorize_replacement`, `abandon`, reason artifact and time. An operator statement remains an attestation. A permitted replacement receives a new command ID with `replacement_of`; it never overwrites the original unknown receipt or claims a vendor acknowledgement.

`CheckpointPolicy` is frozen in the capture/scope configuration: turn-end, pause and cancellation triggers; optional finite interval; maximum checkpoints per attempt; capture byte limits; working-state archive rules; and authorized early-backup behavior. Periodic capture is a bounded host lifecycle action, not an agent-created DAG cycle.
