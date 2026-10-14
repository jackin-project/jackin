# C2: agent sessions and operator control

## Purpose

Control the real CLI through its original Capsule session.
Keep automatic input and operator input separate.
Provide a small adapter for each of the two MVP agents.

## Existing code and changes

Reuse Capsule process ownership, PTYs, rendering, attach transport, and runtime events.
Reuse `SessionSend` for its existing byte-delivery function.
Do not interpret `SessionSent` as agent completion.
Add workflow identity, input ownership, and turn-aware submission around the existing path.
Do not unseal the built-in agent registry or add a dynamic plugin loader.

## API

| Operation | Input | Output |
|---|---|---|
| `describe_agent` | Agent build and selected execution mode | Tested capabilities and unsupported capabilities. |
| `start_session` | Admitted instance, agent selection, workflow owner | `SessionKey` and initial status. |
| `submit` | Session key, command ID, owner epoch, lease generation, expected turn, text reference | Byte receipt or rejection. |
| `wait_turn` | Session key, command ID, expected turn, deadline | Turn event, evidence quality, or timeout. |
| `observe` | Session key and event cursor | Output stream without input authority. |
| `take_control` | Session key and authenticated operator ID | Exclusive lease and new generation. |
| `release_control` | Session key and current operator lease | Released lease; run remains paused until explicit resume. |
| `interrupt` | Session key, valid lease, reason | Recorded interruption result. |

`SessionKey` contains `instance_id`, `capsule_epoch`, and `session_id`.
Record the process start identity as an additional check.
A numeric session ID alone is not sufficient after a Capsule restart.
A vendor conversation ID is optional evidence. It does not replace the physical session key.

## Prompt delivery

Permit only one automatic command in flight for an implementer session.
The host must save the command before it calls `submit`.
Capsule must check the workflow owner and the current input lease before any bytes reach the PTY.
The adapter must encode paste and submit behavior for its tested CLI build.
Do not append Enter or use bracketed paste without the adapter contract.

A receipt contains command ID, session key, written byte count, and lease generation.
It proves only the completed transport operation.
Use a short in-memory command-ID cache in Capsule to reject duplicate calls during that Capsule process lifetime.
The host database provides the durable command record.
Do not add a second Capsule database solely for MVP command receipts.

When a receipt is lost, the host must use `delivery_unknown`.
It must not automatically submit the same command again.
See C5 for the operator resolution procedure.

## Turn detection

Prefer native runtime events for the end of a turn.
Record whether the observation is vendor-reported or inferred.
A tested screen rule may provide a readiness hint.
A readiness hint cannot accept code.
Do not use silence or a fixed sleep as proof that the turn ended.

Keep the current stage blocked when turn status is unknown.
A bounded timeout must produce a reason and preserve the session.
Do not start an unrelated headless conversation to obtain a clearer status API.

## Operator ownership

An observer cannot send keys, paste data, submit text, create a shell, or change shared terminal dimensions.
Render the observed output within the observer's local viewport.

A takeover must atomically replace automatic input ownership.
Reject a second input owner.
Finish a PTY write already in progress before the takeover acknowledgement.
Do not acknowledge takeover while automatic bytes remain queued for later delivery.

Enforce the lease at every input path, including normal attach, control RPC, paste, and new pane actions.
For workflow instances, reject direct unleased `SessionSend` requests.
Do not change ordinary manual instances to require workflow leases.

Pause the run before the operator can change its session.
Increase the candidate generation when manual control begins.
Return control only through the host resume action.
Require a fresh source check before the next acceptance decision.
If an operator disconnects, keep the run paused. Do not transfer control silently.
A stale operator lease must not remain a permanent lock.
Permit the authenticated operator to revoke that lease through the existing control path.
Record revocation before a new lease or resume action.

## Authentication and delegation facts

Record the authentication profile tested by C3.
Record support for initial input, turn observation, follow-up input, observe, takeover, and interruption.
Record support for the `forbid` delegation mode separately.
Do not derive those capabilities from the fact that a binary starts.

Visible subagent events can include child ID, parent ID, task type, and status.
Store an absent ID as unknown.
Do not interpret an implementer's child as an independent reviewer.

## Build instructions

1. Add the session key and workflow control envelope to the protocol boundary.
2. Add two small workflow adapters over the existing runtime adapters.
3. Add input ownership at the common Capsule input boundary.
4. Add command correlation and tested turn observation.
5. Reuse the attach path for observe and takeover.
6. Forward bounded events to C7.

## Local verification

Run K1. Use two real prompts and one independent observer.
Confirm the same session key and process identity before and after the second prompt.
Check rejected automatic input during takeover.
Run the uncertain-receipt case later in K4.

## Dependencies and sources

C3 supplies the isolated instance and credentials.
C5 owns durable command intent. C7 owns the operator API and saved history.
See [SOURCES.md](../SOURCES.md), J6, and the review's session-control findings.
