# State machines, transaction boundaries and crash behavior

**Status:** Proposed normative transition contract. A persisted reducer and transactional effect outbox determine state. Vendor events and operator requests are inputs to that reducer, not direct state mutations.

## Run state machine

| State | Entry condition | Permitted exits |
|---|---|---|
| `planned` | Immutable plan stored; authorization preflight recorded | `running`, `cancelled`, `failed` |
| `running` | Controller lease held; dispatch permitted | `pausing`, `awaiting_operator`, `cancelling`, `reconciling`, `succeeded`, `failed` |
| `pausing` | Pause request durable; no new automation input or node dispatch | `paused`, `awaiting_operator`, `cancelling`, `reconciling` |
| `paused` | Requested pause boundary reached; current activity accurately reported | `running`, `cancelling`, `reconciling` |
| `awaiting_operator` | Approval, unknown delivery, conflict or capability decision needs a human | `running`, `pausing`, `cancelling`, `failed`, `reconciling` |
| `cancelling` | New effects prohibited except authorized stop/preservation/reconciliation | `cancelled`, `reconciling` |
| `reconciling` | Ownership/effect/session state uncertain after restart or loss | Prior safe state, `awaiting_operator`, `failed`, `cancelled` |
| `succeeded` | Mandatory graph outputs and final acceptance/publication contracts satisfied | None; a new run is required for changed work |
| `failed` | A terminal policy/verification/infrastructure outcome has been persisted | Explicit retry continuation may enter `reconciling`; prior disposition remains in history |
| `cancelled` | Cancellation recorded and work preserved or safely held | None; resuming work creates a new linked run |

A terminal run may still have preserved instances and cleanup obligations. The run records `retained_resources` and `cleanup_status` separately. Failure reporting never waits for successful destructive cleanup. Explicit retry of a failed run appends a continuation generation and new attempts; it never rewrites old attempts or resets lifetime repair/budget counters.

## Node-attempt state machine

`pending → ready → provisioning → running → collecting → succeeded` is the ordinary path. Other states are `waiting_input`, `waiting_operator`, `reconciling`, `failed`, `cancelled` and `skipped`. Only the scheduler can move pending to ready after dependency and budget admission. Agent completion moves an agent node toward collection; it never directly marks an implementation accepted.

An individual attempt’s terminal result is immutable. Infrastructure retry creates a new attempt number. Repair creates a new iteration in a defined repair scope while reusing the logical implementer session when supported. A skipped node produces a typed `NotProduced` result, never fake success. Downstream requirements explicitly allow or reject that absence.

## Durable transition algorithm

1. Authenticate the input and load the run/node version and controller epoch.
2. Validate the transition, permissions, deadline, candidate revision and input lease where relevant.
3. In one database transaction, compare the expected version, append the event, update materialized state and enqueue external-effect intent with a unique deduplication key.
4. Commit durably. Only after commit may an effect dispatcher execute the outbox item.
5. Persist the effect receipt and follow-on event in a new transaction. A lost reply causes lookup/reconciliation, not an assumed failure or success.

The state reducer is deterministic with clock, policy snapshots, random identifiers and external observations supplied explicitly as recorded inputs. It performs no network calls, filesystem mutation, PTY writes or model calls itself. Use a single controller owner locally; fencing still protects against stale processes after daemon restart.

## Prompt delivery states

```text
queued → dispatching → capsule_recorded → bytes_written
                                 ↘ delivery_unknown
bytes_written → agent_acknowledged → turn_active → turn_completed
      ↘ delivery_unknown
```

Not every adapter supports `agent_acknowledged`; capabilities explicitly say so. `bytes_written` means only transport delivery. A semantic vendor acknowledgement may establish receipt, not implementation correctness. The terminal-output stream is never parsed into an authoritative acceptance decision.

The PTY and a database transaction cannot atomically commit one prompt together. Capsule records a command intent before writing. A crash between writing bytes and recording the final receipt may leave the outcome unknown. **The required guarantee is no silent duplicate prompts, not a fabricated universal exactly-once API.** Unknown commands block automatic resend. Recovery queries a durable Capsule receipt or certified vendor message record; without proof, the operator chooses a logged resolution. A partial paste is not safely retried as a full prompt.

The host outbox and Capsule journal use the same command ID and payload digest. Duplicate ID plus identical digest returns the stored receipt without new input. Duplicate ID plus different payload fails. This reduces duplicate writes but does not remove the process-crash ambiguity; the state machine must retain it.

## Pause, takeover and cancellation

Default pause is **drain**: stop new nodes and automated input, allow already-running turns to reach a safe boundary, then report paused. A pause request does not falsely claim the worker stopped immediately. `interrupt` pause requests a certified interrupt and waits for actual process/turn observations. A backend suspend/freeze is a separate capability, not a synonym for pausing the scheduler. Absolute run deadlines continue during pause; active-runtime budgets charge only actual activity.

Takeover requests fence automation before granting an operator lease. An already-active agent turn may continue until interrupted; the UI must show that fact. Releasing the lease does not automatically resume the workflow: re-evaluate session state, candidate revision, deadline, policy and unverified manual edits. A terminal disconnected operator is not automatically assumed to have relinquished control safely.

Cancellation is durable intent, then bounded interrupt/terminate of worker process groups and evidence preservation. Unknown remote publication remains reconcilable after cancellation. No new publishing action is dispatched after cancellation wins the compare-and-set; an already-authorized in-flight action may complete and must be recorded, never hidden.

## Recovery matrix

| Crash/loss point | Recovery action | Forbidden shortcut |
|---|---|---|
| Before transition commit | Read old state; no effect should have been dispatched | Infer intent from a half-written UI message |
| After commit, before backend create | Dispatch the persisted effect once under its owner epoch | Create an untracked instance first |
| After backend create, before receipt | Find exact workflow/attempt/effect labels and adopt one matching instance; conflicts require reconciliation | Blindly create a second writable worker |
| After prompt bytes, before ack | Consult journal/vendor receipt; otherwise `delivery_unknown` and stop automatic input | Resend because a timeout occurred |
| During checkpoint capture | Reject incomplete seal; retain original state and finish/redo under a new barrier if safe | Treat a partially copied working tree as an immutable revision |
| After blob fsync, before DB reference | Treat blob as recoverable orphan; reconcile before eventual GC | Delete a blob protected by an unfinished intent |
| After DB reference, blob missing/corrupt | Integrity failure; block affected acceptance and retain remaining evidence | Report success from cached metadata |
| During test/review | Inspect authenticated worker identity and target revision; resume collection or create bounded new attempt | Attribute late results to whichever revision is current |
| After remote push/PR create, before receipt | Read exact remote ref or query run-specific PR marker; reconcile duplicates explicitly | Repeat a non-idempotent creation blindly |
| During cleanup | Read preservation holds and removal journal; continue only proven-safe owned removal | Assume terminal run means resources are disposable |
| DB unavailable/storage full | Stop new effects and input, preserve bounded local journals, expose health failure | Continue autonomous mutations from RAM |

## Approval and current-candidate races

Store `current_candidate_ordinal` per candidate scope. Every newly sealed candidate increments it. A test/review result binds an exact checkpoint and may remain valid historically, but cannot satisfy a newer candidate. Acceptance performs a database CAS on this ordinal and the evidence set. Publication rechecks that certificate and current policy immediately before dispatch. A Git integration/merge commit creates a new candidate and needs its own verification even when both inputs passed independently.

## Loop semantics

The user graph remains acyclic. A `RepairScope` owns a fixed implement/checkpoint/verify/review/gate template and an optional feedback edge expressed as controller metadata. An iteration expands the same declared template with a new iteration number; it is not an arbitrary backward dependency. `max_repairs = 2` permits the initial evaluation plus at most two repair iterations. Counters persist across pause, retry and daemon restart. Late outputs from earlier iterations remain attributable but cannot advance the current iteration.

## Scope-local waits and finalization fence

A node waiting for operator input does not necessarily block independent authorized branches. The aggregate run enters `awaiting_operator` when no safe autonomous progress remains or the policy explicitly pauses all work; otherwise it may remain `running` with a `waiting_operator` node. A global `paused` state prohibits new dispatch. UI projections must distinguish these cases.

Before final acceptance, the host establishes a finalization write barrier and confirms the current working-state manifest matches the tested/reviewed candidate. If it does not, it creates a new candidate and reruns required checks. The barrier/fence remains effective through acceptance and publication reconciliation. Reopening mutation requires a recorded invalidation first. A filesystem watcher alone is not sufficient proof of no unsealed changes.

Capsule's receipt state `recorded` projects to host command state `capsule_recorded`. An adapter without semantic acknowledgement may record a later correlated turn observation only under its certified contract; it must not fabricate an acknowledgement to fill a state-machine gap.
