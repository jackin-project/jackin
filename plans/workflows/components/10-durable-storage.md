# C10 — Durable storage, event ledger and effect outbox

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Persist authoritative workflow state, immutable events and external-effect intents. Implement `WorkflowStore` in the proposed `jackin-workflow-store` service crate, with an independent workflow database and schema.

## 2. Existing foundation and gap

Jackin engineering rules require `turso` for general-purpose embedded database access. The narrow `jackin-omp-store` rusqlite exception is not permission for a new workflow binding. Reuse the workspace dependency and approved connection patterns, but do not couple workflow state to usage-domain tables. Required crash and durability guarantees remain unverified until CERT-STORE. [J18](#evidence-and-references).

## 3. API and transaction contract

```text
put_plan(immutable_envelope, source_refs) -> PlanRef
create_run(plan_ref, caller, idempotency_key) -> RunRecord
apply_transition(expected_versions, events, state_changes, outbox_intents) -> CommitReceipt
claim_effect(effect_id, controller_epoch, deadline) -> EffectLease
record_effect_receipt(effect_id, lease, outcome) -> CommitReceipt
read_snapshot(run_id) -> RunSnapshot
read_events(run_id, after_seq, limit) -> EventPage
acquire_controller(owner, lease) -> ControllerEpoch
migrate(expected_schema, target_schema) -> MigrationReceipt
```

Each `apply_transition` is atomic across materialized state, event append and outbox insertion. Uniqueness constraints enforce caller idempotency keys, per-run sequences, attempt addresses and external effect deduplication. Optimistic version checks plus a local controller lease prevent stale updates; database lock contention is not permission to bypass CAS.

## 4. Required storage model

| Table / collection | Key and critical invariant |
|---|---|
| `workflow_plans` | plan_hash; canonical bytes and input refs immutable |
| `workflow_runs` | run_id; plan FK, state/version, current controller and lifetime counters |
| `node_attempts` | attempt_id; unique run/node/iteration/attempt tuple; terminal records immutable |
| `run_events` | unique run/seq and event_id; append-only logical history |
| `session_bindings` | logical_session_id plus physical binding generation; epoch/vendor IDs distinct |
| `input_commands` | command_id; immutable payload digest, target and expected lease/turn |
| `input_receipts` | command_id/receipt_seq; known versus unknown delivery retained |
| `effect_outbox` | effect_id plus unique dedup key; desired action immutable, dispatch state versioned |
| `effect_receipts` | effect_id/receipt_seq; remote/runtime observations retained |
| `checkpoints` | checkpoint_id; producer, ordinal, OIDs, seal and artifact refs immutable |
| `candidate_heads` | run/scope; latest ordinal/checkpoint updated by CAS |
| `verification_results` | verification_id; checkpoint and trusted harness/policy digest required |
| `review_findings` | finding_id; immutable original content with appended disposition history |
| `operator_decisions` | decision_id; principal, exact target, action, reason, expiry and version |
| `acceptance_certificates` | certificate_id; exact checkpoint/evidence/decision set; revocation state separate |
| `publication_records` | publication/effect ID; remote locator and reconciliation receipt |
| `artifacts` and `artifact_refs` | content identity, encryption/integrity metadata and ownership references |
| `preservation_holds` | hold ID; protected resource closure and release rules |
| `resource_reservations` | attempt/resource key; acquired/released under scheduler transactions |
| `schema_migrations` | version; checksum and completion record; one migration owner |

Exact SQL column types must match the canonical contracts and turso-supported APIs. Foreign-key/integrity constraints or equivalent validated transactions are mandatory. Do not silently disable an unsupported database feature and still claim the guarantee.

## 5. Durability, performance and recovery

Use owner-only local storage. Select and verify the pinned engine’s supported durable transaction/journal mode and read back settings when available. Do not assume SQLite WAL documentation proves identical turso behavior. Avoid network filesystems for the initial local ledger unless separately certified. Large terminal streams, prompts and diffs belong in artifact storage, not unbounded event rows.

The database/filesystem boundary uses C11’s seal-before-reference protocol. Reconcile orphan sealed blobs and unfinished intents before garbage collection. Database references must never point at an unsealed blob. Acknowledged acceptance requires all essential evidence durably referenced and integrity-verified.

On storage full/unavailable, halt new effects and automated input. Keep bounded protected local journals for already-in-flight Capsule receipts; surface health failure. Recovery is reconciliation, not replay of every outbox row as if it were new. On corruption, preserve the database and resources, restore a verified backup if available, and mark unknown intervals explicitly; do not synthesize missing history.

Provide consistent backups through a supported engine snapshot/backup boundary, plus artifact manifests. Never copy a live database file alone and call it a reliable backup. Migrations require exclusive ownership, preflight free space, backup/rollback strategy and interrupted-migration tests. Materialized read models must be rebuildable from the retained authoritative history and plan.

## 6. Build instructions

Implement store trait and migration tests first, then atomic transitions, outbox leases and event cursors. Use the workspace turso dependency. If a common DB facade is extracted from existing services, remove domain coupling rather than importing usage business logic. Introduce killpoints around every commit/receipt boundary and run them against the real engine, not only an in-memory fake. Add backup/restore and retention-reference audits before production enablement.

## 7. Dependencies and integration

Domain types from `jackin-workflow-spec`; governed telemetry for bounded operations; C11 artifact protocol. No UI, Capsule binary or Docker dependency. A future PostgreSQL implementation may implement the port, but v1 has one local controller and no distributed-scheduling claim.

## 8. Acceptance tests

`DB-01`: state/event/outbox commit all-or-none. `DB-02`: stale versions/controller epochs fail. `DB-03`: process kill and power-loss harness preserve acknowledged transitions. `DB-04`: migration interruption and backup/restore retain evidence. `DB-05`: disk-full stops effects. `DB-06`: event pagination/rebuild is deterministic. `DB-07`: orphan/missing/corrupt artifacts cannot produce false acceptance. `DB-08`: no unapproved general-purpose SQLite binding is introduced.

## Evidence and references

- **J18 — [Engineering rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/ENGINEERING.md).** Full document. Rust-first, reuse, turso general-purpose DB, governed jackin-telemetry and privacy rules.
- **E12 — [SQLite write-ahead logging](https://www.sqlite.org/wal.html).** Official SQLite WAL durability/placement considerations; do not assume the pinned turso engine behaves identically. Accessed 2026-10-14.

## Additional required records

Persist `finalization_records` keyed by run/scope/finalization generation, with checkpoint/manifest/barrier proof and active/revoked status. Persist `delivery_resolutions` keyed by resolution ID and original command, with actor/reason/decision and optional replacement command. Both participate in version-checked event/state transactions. Their existence is required before final acceptance or operator-authorized replacement input respectively.
