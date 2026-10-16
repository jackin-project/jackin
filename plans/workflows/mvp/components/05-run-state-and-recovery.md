# C5: durable run state and recovery

## Purpose

Control one fixed workflow on the host.
Save enough state to recover after a host daemon restart.
Do not infer workflow state from the agent conversation.

## Existing code and changes

Add `jackin-workflow` with a small `store` module.
Use the existing `turso` dependency and approved connection pattern.
Keep workflow tables separate from usage tables.
Inject the workflow service into the existing host daemon.
Do not make the daemon import a runtime umbrella that already imports the daemon.

## API

| Operation | Input | Output |
|---|---|---|
| `start_run` | Stored plan hash and idempotency key | Existing or new run ID. |
| `advance_run` | Run ID, expected row version, observed result | Saved next state and required effect. |
| `record_candidate` | Run ID, candidate ID, expected row version | Saved candidate and bumped generation. |
| `record_session` | Run ID, expected row version, worker and session identity | Saved session attachment. |
| `record_checkpoint` | Run ID, expected row version, candidate, verified commit/tree, source manifest, local path | Saved checkpoint after source verification. |
| `record_test` | Run ID, expected row version, frozen check script and bundle digest | Host-observed verdict in a pinned worker. |
| `record_review` | Run ID, expected row version, isolated review file, recorded policy hash | Host-validated verdict bound to the policy. |
| `record_backup` | Run ID, expected row version, remote and ref | Backup effect done after ref comparison. |
| `accept_run` | Run ID, expected row version, expected policy hash | Accepted only on passing tests, acceptable review, current source, completed backup. |
| `pause_run` | Run ID, actor, reason | Paused state. |
| `resume_run` | Run ID, actor, current input state | Active state or blocking reason. |
| `cancel_run` | Run ID, actor, reason | Cancelled state and preserved work references. |
| `recover_runs` | Current owner epoch and instance inventory | Recovery results for incomplete runs. |
| `resolve_input` | Run ID, command ID, operator decision | Recorded decision and safe next stage. |

Only the host service can apply state transitions.
Workers can submit observations. They cannot mark a run accepted.

Serve the daemon socket from a per-user local run directory with owner-only permissions.
Treat the socket as local host transport, not a trust boundary.
Treat actor strings as self-attested labels on every daemon RPC.
Check run positions and recorded evidence, never the actor label.
Expose the daemon socket to no worker mount.

## Storage

Use the smallest schema that supports these records:

| Table | Required data |
|---|---|
| `runs` | Run ID, plan reference, status, stage, round, candidate ID, candidate generation, row version, owner epoch, deadline. |
| `sessions` | Run and worker IDs, instance ID, Capsule epoch, session ID, process identity, agent and auth profile references. |
| `commands` | Unique command ID, target session, prompt reference, turn, lease generation, state, receipt, and operator resolution. |
| `checkpoints` | Candidate identity, exact commit and tree, source manifest, local path, backup state, preservation hold. |
| `evidence` | Candidate ID, kind, source, quality, result, policy hash, artifact digest. |
| `events` | Run ID, monotonic sequence, time, actor, event kind, evidence reference. |
| `effects` | Unique operation ID, typed operation, expected state, status, exact external identity, preservation data. |

An effect is an external action, such as launch, capture, or backup.
Use a closed Rust enum for these effects.
Do not build a generic distributed outbox framework.
Store prompt and output content in protected files. Keep their digests in the database.

Save a state change, its event, and its next effect intent in one transaction.
Write artifact files through temporary files, flush them, and rename them before recording completed references.
Verify referenced content on recovery.
Use the database's actual supported transaction and durability settings.
Do not claim power-loss guarantees that the selected database mode has not established.
The MVP proof covers process interruption and restart.

## State model

Keep run status separate from the current stage.

```text
status: active | paused | needs_operator | accepted | failed | cancelled
stage:  prepare | implement | capture | test | review | decide
round:  0 through maxRepairTurns
```

A new valid run starts at `active / prepare / 0`.
Only passing tests, passing review, and a current-source check can set `accepted`.
Code failures can start another implementer turn while the repair budget remains.
Infrastructure failures cannot masquerade as code findings.
A lost session or uncertain prompt sets `needs_operator`.
Terminal states retain their evidence and preservation holds.

Use one local service-owner lock and one active-run slot.
A second start request returns `busy`, unless its idempotency key identifies the existing run.
Do not add a queue in the MVP.
Increment the owner epoch when the host service restarts.
Reject old owner epochs on later automatic input.

## Command states

```text
queued -> dispatching -> written -> turn_ended
                 \----> delivery_unknown
```

`queued` means no dispatch intent has begun.
Commit `dispatching` before the transport call.
Set `written` only from the matched Capsule receipt.
Set `turn_ended` only from a correlated observation, with its evidence quality.
Do not equate any command state with code acceptance.

If the host crashes in `dispatching`, the command can be delivered or undelivered.
Recover it as `delivery_unknown`.
Do not call `submit` again automatically.

## Recovery actions

| Found state | Required action |
|---|---|
| Planned launch, matching owned instance exists | Adopt its exact identity. Do not create a duplicate instance. |
| Planned launch, no matching instance exists | Confirm backend absence before a new launch attempt. |
| Matching live session and confirmed written command | Reconnect to that session. Wait or check its observed state. |
| Uncertain input delivery | Block automatic input and request operator action. |
| Capsule epoch or process identity changed | Record `session_lost`. Keep source. Do not claim same-session continuation. |
| Capture left an instance paused | Inspect capture state. Complete or abandon staging safely, then resume or preserve the instance. |
| Test or review result is incomplete | Discard partial evidence. Start at most one fresh verification attempt at the same checkpoint. |
| Backup response is uncertain | Query the exact remote ref. Do not push without comparison. |
| Candidate changed | Invalidate old acceptance eligibility. Verify the new candidate within the remaining limit. |
| Storage cannot save a required record | Stop new actions, preserve instances, and report the storage failure. |

Use operation IDs and ownership labels for comparison.
Do not adopt an instance because its display name looks similar.

## Operator resolution

The operator must inspect the original session before resolving uncertain delivery.
Offer these explicit actions:

| Action | Meaning |
|---|---|
| `continue_from_current` | Do not send old input. Use observed current session state, then capture and verify current source. |
| `submit_new` | Record a new operator instruction with a new command ID. Send it only after input admission succeeds. |
| `cancel` | End the run and preserve work. |

Record the actor and reason.
Keep the original command marked uncertain, with a link to its resolution.
An operator decision does not replace tests or review.
Do not allow `resume` alone to erase uncertainty.

## Pause and cancellation

A normal workflow pause stops new automatic actions.
An active agent turn can finish within its existing deadline.
A checkpoint pause is different: C4 briefly freezes the worker for source capture.
Show these states separately.

Cancellation blocks new actions, interrupts active work, and records the final disposition.
Escalate through the existing runtime stop path when interruption fails.
Preserve source before any normal cleanup.
Do not terminate the original CLI merely to deliver feedback.

## Build and local verification

Implement the tables, reducer, and one effect dispatcher before adding more abstractions.
Run K4 with one normal recovery case and one lost-receipt case.
Reuse C4's pending capture fixture for pause recovery.
Use K7 for the real host daemon interruption.

## Dependencies

C1 provides plans. C2 provides sessions. C3 and C4 implement external actions.
C6 provides verification decisions. C7 is the only client-facing control service.
