# C09 — Scheduler, dependencies and resource budgets

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Execute the compiled graph through deterministic, durable transitions while respecting dependencies, resource budgets and cancellation. Own the scheduler/reducer in `jackin-workflow`; keep process, database and network effects behind ports.

## 2. Existing foundation and gap

Existing research describes a deterministic run ledger and evidence-based transitions, but the inspected daemon API is a lifecycle shell rather than a workflow scheduler. Implement the coordinator as a native host service, not a prompt that asks one agent to manage other agents. [J04](#evidence-and-references), [J10](#evidence-and-references).

## 3. API and data contracts

```text
start_run(plan_hash, caller, idempotency_key) -> RunId
reduce(run_snapshot, recorded_input) -> TransitionBatch
admit_ready_nodes(run, budget_snapshot) -> [AttemptAdmission]
request_pause(run, mode, expected_version) -> Operation
request_resume(run, expected_version) -> Operation
request_cancel(run, reason, expected_version) -> Operation
retry_failed(run, target, expected_version, permit) -> Continuation
```

`TransitionBatch` contains expected state versions, events, updated counters and effect intents, with no side effects performed. `RunBudget` includes max concurrent instances/agent turns, CPU millicores, memory bytes, writable storage reservation, active runtime, absolute deadline, infrastructure retries, repair count and optional spend/token limits. Every field is finite or explicitly disallowed by host policy; unbounded limits are not implicit defaults.

## 4. Required functionality

Topologically validate the main DAG and bounded repair expansion. A ready node requires satisfied control dependencies, valid typed inputs, current authorization and resource admission. Use stable node-ID ordering for equally ready nodes and a documented fair queue across runs; actual external event arrival order is recorded, not guessed from timestamps.

Reserve resource maxima transactionally before dispatch. Count all allocated workers, including independent reviewers/verifiers and any permitted sidecars; release only after authoritative stop/reconciliation. Avoid deadlock when a retained implementer and its verifier cannot fit together: compiler/admission must prove the chosen phase overlap fits, or require a certified suspend/checkpoint strategy. Never silently kill a session required for feedback just to make room for a reviewer.

Conditions evaluate only typed evidence. A false branch marks scoped nodes skipped with an explicit reason. Missing output is `NotProduced`, not an empty success. Joins declare `all_selected`, `all_required` or an explicitly permitted success threshold; required acceptance paths cannot use quorum/any semantics to discard a failed mandatory check. A fail-fast parallel group cancels unfinished siblings but still preserves their outputs and resolves in-flight effects.

Infrastructure retry creates a new attempt only when policy permits and prior effects are reconciled. Repair is a separate controller budget. The run’s lifetime counters survive operator retry/resume. A change to the immutable plan requires a new linked run; “retry” is not editing the plan in place.

Absolute wall-clock deadline continues during paused/operator states. Active runtime tracks observed running periods. Spend/token data may be delayed or estimated: record precision and provenance, reserve before new turns, and label estimated limits honestly. A hard monetary ceiling requires an enforceable provider/broker mechanism; if only delayed usage is available, reject a request for guaranteed hard spend enforcement. Model calls already in flight can consume beyond a soft estimate.

## 5. Failure and state ownership

A node timeout stops its process group through the runtime and records the result independently of cleanup. A successful command/test/review node result is not whole-run acceptance. `succeeded` requires every mandatory output and gate, including publication if the plan requires it. An Agent node may complete a bounded turn while its retained session continues to exist for feedback or inspection.

No scheduler task writes the database or PTY outside the authoritative transaction/effect path. Tokio tasks use bounded channels, explicit cancellation tokens and ownership guards. Panics/worker-task loss become recorded infrastructure outcomes, never a dropped future interpreted as success.

## 6. Build instructions

Start with a pure reducer, fake clock, fake execution platform and in-memory test store. Add property tests for dependencies, budgets, skipped branches, retry counters and cancellation races. Implement transactional admission and outbox dispatch next. Integrate real runtime ports only after simulated fault cases pass. Add repair-scope expansion using C14’s exact semantics.

## 7. Dependencies and integration

C01–C02 provide compiled nodes; C03 grants; C04/C06 effect ports; C10 atomic state; C14 repair; C15 acceptance; C17 recovery. The host daemon owns the Tokio supervision tree and lifecycle.

## 8. Acceptance tests

`SCH-01`: no node starts before dependencies/inputs. `SCH-02`: resource reservations never exceed host/run budgets. `SCH-03`: independent nodes run concurrently. `SCH-04`: conditional skips and fail-fast cancellation preserve mandatory gates. `SCH-05`: retries do not reset repair/deadline counters. `SCH-06`: retained implementer plus verifier cannot deadlock admission. `SCH-07`: unknown hard spend enforcement fails honestly. `SCH-08`: cancellation racing completion has one persisted outcome.

## Evidence and references

- **J04 — [Existing orchestration design constraints](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/docs/content/research/agents/orchestration/workflow-systems/agent-workflow-orchestration/02-design-constraints.mdx).** Full document. Proposed deterministic ledger, evidence, operator intervention and Capsule ownership.
- **J10 — [Host daemon shell](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-host-daemon/src/host_daemon.rs).** Lines 1–260. Protocol version 2; Hello, Status, TelemetryHealth, AttentionSnapshot and Shutdown request variants. Not a workflow RPC service.
- **J18 — [Engineering rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/ENGINEERING.md).** Full document. Rust-first, reuse, turso general-purpose DB, governed jackin-telemetry and privacy rules.

## Reservation and finalization clarification

Suspending a process does not release its memory/disk reservation. Reclaim only resources actually released under a certified lifecycle operation. Include retained implementers in admission calculations while tests/reviews run. Finalization is a host-coordinated writer fence; it is not inferred from a quiet terminal or a successful test of an older snapshot.
