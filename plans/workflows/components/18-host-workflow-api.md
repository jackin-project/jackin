# C18 — Versioned host workflow service

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Expose one authoritative workflow service for CLI, Console and visual authoring. Add it to the existing host-daemon composition with version negotiation and authenticated local transport; do not put a separate workflow engine in each client.

## 2. Existing foundation and gap

The inspected daemon shell uses protocol version 2 and a 16 KiB request limit, with lifecycle/status/attention operations. It does not expose workflow methods. `jackin-host` is desktop integration, not the daemon crate. Preserve the existing protocol while adding an explicitly negotiated workflow surface. [J10](#evidence-and-references), [J11](#evidence-and-references).

## 3. API envelope

```json
{
  "api_version": "workflow.v1",
  "request_id": "request-example-001",
  "method": "runs.pause",
  "idempotency_key": "operator-pause-example-001",
  "params": {"run_id": "run-example-001", "expected_version": 12, "mode": "drain"}
}
```

This is the proposed workflow-service payload, not a currently accepted daemon request. A reviewed daemon protocol extension negotiates `workflow.v1`; legacy protocol-2 requests retain their existing handler. Reuse the socket/lifecycle infrastructure and framing conventions where compatible. Large Pkl, prompts and artifacts use references/chunked artifact transfer rather than increasing the old control limit silently.

Responses are `{api_version, request_id, result}` or `{api_version, request_id, error: WorkflowError}`. Long operations return an `operation_id`, current state and event cursor; an accepted operation is not a completed workflow. Mutations require authenticated principal, idempotency key and expected state version except initial creation. Reusing a key with different payload returns conflict.

## 4. Required methods

| Method | Inputs | Output / authorization |
|---|---|---|
| `definitions.validate` | source/artifact refs, parameters, schema version | Diagnostics; authorized source reads only |
| `definitions.plan` | validated inputs and host policy context | Immutable plan hash and effective-permission report |
| `capabilities.list` | backend/agent filters | Supported/unsupported/unverified capabilities and certificates |
| `runs.start` | plan_hash, idempotency key | Run ID/operation; launch authority |
| `runs.list` | filters, cursor, page size | Sanitized run summaries and next cursor |
| `runs.get` | run_id | Snapshot, version, cursor, retained resources |
| `runs.graph` | run_id or plan_hash | Compiled graph/source mapping plus state projection |
| `runs.events` | run_id, after_seq, filters | Resumable event stream; explicit history-gap behavior |
| `runs.pause` / `runs.resume` | run_id, expected_version, pause mode if relevant | Operation and transition assessment |
| `runs.cancel` | run_id, expected_version, reason | Durable cancel operation; no implicit deletion |
| `runs.retry` | failed run/node target, expected_version, retry permit | Linked continuation/new attempt records |
| `gates.approve` / `gates.reject` | request ID, exact checkpoint, expected versions, reason | Scoped operator decision |
| `sessions.attach` | run/node/session, observe/control intent | Short-lived attach descriptor for exact Capsule address |
| `sessions.acquire_control` / `renew_control` / `release_control` | expected generation and lease fields | Fenced lease/receipt |
| `commands.resolve_delivery` | run/command ID, expected versions, scoped operator resolution and reason | Audited received/replacement/abandon decision; never an invented vendor acknowledgement |
| `runs.export` | run_id, format, redaction/classification policy | Export operation and artifact manifest |
| `artifacts.read` / authorized import | artifact or approved source reference, bounds | Verified content/immutable artifact ref |
| `definitions.preview_edit` / `apply_edit` | source hash, supported structured patch | Diff/diagnostics; CAS-persisted source bundle |

No generic host-shell method, arbitrary host-file write method or agent-issued approval endpoint is included.

## 5. Security and stream semantics

Default transport is a per-user Unix socket with restrictive permissions and peer identity checks. Worker namespaces do not receive the broad host workflow socket; they use narrowly scoped Capsule/runtime channels. Apply authorization to every method and referenced run/artifact, not merely on initial connection. Limit message size, concurrent subscriptions, upload quotas and filter complexity.

For a browser editor, use a host-local authenticated bridge with explicit origin allowlist, CSRF protection, short-lived session credentials and no wildcard CORS. Do not place tokens in URL query strings. The bridge delegates to the same service and cannot change plan or gate semantics. An unauthenticated loopback listener is not sufficient security.

Event streams deliver at least once; clients deduplicate `(run_id, seq)`. Snapshot replies include the associated event cursor to avoid a snapshot/subscription race. Slow clients receive bounded buffering and reconnect/gap instructions rather than unbounded memory growth. Sensitive payloads require separately authorized artifact reads.

## 6. Build instructions

Define wire types in the low-level protocol/domain boundary and generate fixtures. Add host service composition, supervised controller lifecycle and compatibility handshake. Implement methods as adapters to existing domain services, not separate logic. Add local peer authorization and limits before browser bridging. Keep blocking backend/DB operations off the daemon’s request-processing path through the existing async/process abstractions.

## 7. Dependencies and integration

C02 planning, C09 control, C10 queries, C07 attachment, C15 approval, C21 editing and C22 export. C19/C20 and web UI consume exactly these operations. The workflow service survives clients disconnecting.

## 8. Acceptance tests

`API-01`: all clients observe the same run/version. `API-02`: old daemon requests remain compatible; unsupported workflow capability is explicit. `API-03`: peer/run/artifact authorization enforced. `API-04`: duplicate key/different body conflicts. `API-05`: cursor reconnect has no silent gaps. `API-06`: oversized payloads and slow subscribers are bounded. `API-07`: browser CSRF/origin and worker socket access tests fail safely. `API-08`: API cannot run arbitrary host shell commands.

## Evidence and references

- **J10 — [Host daemon shell](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-host-daemon/src/host_daemon.rs).** Lines 1–260. Protocol version 2; Hello, Status, TelemetryHealth, AttentionSnapshot and Shutdown request variants. Not a workflow RPC service.
- **J11 — [Host desktop integration crate](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-host/src/lib.rs).** Full file. Clipboard and desktop integration; not the daemon ownership location despite its name.
- **J18 — [Engineering rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/ENGINEERING.md).** Full document. Rust-first, reuse, turso general-purpose DB, governed jackin-telemetry and privacy rules.
