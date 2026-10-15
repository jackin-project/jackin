# C7: host API, CLI, Console, and history

## Purpose

Expose one workflow service through Jackin's existing interfaces.
Let the operator see the current run, inspect evidence, and control the original session.

## Existing code and changes

Extend the existing host daemon transport and CLI dispatch.
Add a small Console list and detail view.
Reuse Capsule rendering and attachment.
Do not add a web server, browser graph editor, or second workflow engine.

## Host API

Use a versioned local request envelope:

```json
{
  "api_version": 1,
  "request_id": "request-001",
  "method": "workflow.status",
  "params": {"run_id": "run-001"}
}
```

Mutating calls also require an idempotency key.
The host authenticates the local peer before it reads supplied paths or applies a change.
Use the existing local socket and peer-authentication policy.
Negotiate the workflow API version separately from unrelated daemon messages.

| Method | Required parameters | Result |
|---|---|---|
| `workflow.validate` | Source path or staged input reference | Diagnostics. No worker launch. |
| `workflow.plan` | Source reference and host profile selection | Plan hash, summary, and granted limits. |
| `workflow.run` | Plan hash, idempotency key | Run ID and initial status. |
| `workflow.list` | Optional status and cursor | Bounded run list and next cursor. |
| `workflow.status` | Run ID | Status, stage, round, candidate, sessions, backup state, and blocking reason. |
| `workflow.graph` | Run ID | Fixed stage list with node states and checkpoint references. |
| `workflow.events` | Run ID, after-sequence cursor | Durable event stream with resume cursor. |
| `workflow.pause` | Run ID and reason | Paused run. |
| `workflow.resume` | Run ID and reason | Active run or unresolved blocking reason. |
| `workflow.cancel` | Run ID and reason | Cancelled run and preserved-work references. |
| `workflow.attach` | Run ID, worker, observe or control mode | Exact session key and scoped attachment data. |
| `workflow.resolve_input` | Run ID, command ID, action, reason, optional new instruction reference | Audited decision from C5. |
| `workflow.export` | Run ID, destination, requested content class | Export manifest and Markdown report path. |

Use typed errors: `invalid_request`, `unsupported_version`, `not_found`, `busy`, `denied`, `state_conflict`, and `needs_operator`.
Include a stable code, safe message, and corrective action.
Do not include raw secrets or internal credential locations.

## CLI

All commands below are MVP target commands.

```text
jackin workflow validate FILE
jackin workflow plan FILE --json
jackin workflow run --plan HASH
jackin workflow status [RUN] --json
jackin workflow graph RUN
jackin workflow events RUN --after SEQ
jackin workflow pause RUN --reason TEXT
jackin workflow resume RUN --reason TEXT
jackin workflow cancel RUN --reason TEXT
jackin workflow attach RUN --worker implementer --observe
jackin workflow attach RUN --worker implementer --control
jackin workflow resolve-input RUN COMMAND --action ACTION --reason TEXT [--instruction FILE]
jackin workflow export RUN --out DIRECTORY
jackin workflow record session RUN --worker WORKER --expected-version V [--instance INSTANCE ...]
jackin workflow record candidate RUN --candidate CANDIDATE
jackin workflow record checkpoint RUN --candidate CANDIDATE --commit COMMIT --manifest MANIFEST --local-path PATH --expected-version V
jackin workflow record test RUN --check-script PATH --bundle-digest DIGEST --expected-version V
jackin workflow record review RUN --review-file PATH --expected-version V
jackin workflow record backup RUN --remote REMOTE --ref REF --expected-version V
jackin workflow record advance RUN --observation OBSERVATION [--actor ACTOR]
```

`status` without a run ID lists runs.
`resolve-input` actions are `continue_from_current`, `submit_new`, and `cancel`.
`submit_new` requires an instruction file.
The host freezes its content before delivery.

Use Capsule's normal explicit detach action to release operator control.
The control client must call C2 `release_control` before it detaches.
The host records that release and leaves the run paused.
Then use `workflow resume` to return control to the scheduler.
A lost connection is not an explicit release.

Return exit code 0 for a completed control request.
Return 2 for invalid input and 3 for denied access or unsupported capability.
Return 4 for a state conflict or required operator action.
Return 1 for an execution or transport error.
Starting a run does not mean that the run passed.
The status response must show that distinction.

Do not add `retry`, PR publication, merge, or general approval-gate commands to this MVP.
Use a new run for a new plan or a lost agent session.

## Console

Add one workflow list with run ID, stage, status, and blocking reason.
Add one detail view with the fixed graph, current candidate, findings, and backup state.
Provide observe, take control, resume, cancel, and export actions.
Show preserved work and its location through the existing safe path display.

The graph is a view of saved state.
It cannot edit the workflow or override a host decision.
Observe and takeover must open the exact Capsule session from the run record.
A copied transcript is not a live attachment.

## Event history

Save these event types when their source exists:

```text
plan_saved
worker_started
prompt_dispatch_started
prompt_written
prompt_delivery_unknown
turn_started
turn_ended
test_started
candidate_recorded
operator_control_acquired
operator_control_released
vendor_child_reported
tool_activity_reported
sandbox_denial_reported
resource_limit_reached
provider_failure_reported
checkpoint_created
checkpoint_backup_changed
test_completed
review_completed
feedback_submitted
candidate_invalidated
run_accepted
run_failed
run_cancelled
recovery_completed
observation_gap
```

Each event has a run sequence, timestamp, actor, source, evidence quality, and applicable session or candidate IDs.
Use `observed`, `reported`, `inferred`, or `unknown` for evidence quality.
The host assigns source and quality. Untrusted payloads cannot promote themselves to trusted observations.

A provider failure can identify authentication, unavailable model, usage limit, or transport failure.
Do not assign a cause without evidence.
A denial event requires a runtime or policy observation.
Do not promise a log of every kernel denial when the backend lacks that observation.

Keep the exact requested binary, model, sandbox, and input digests in startup evidence.
Keep vendor-reported identity separate from requested identity.
Link child events to their known parent. Leave unknown lineage explicit.
Do not record or reconstruct private model reasoning.

## Terminal history and export

Use a bounded Capsule spool for protected session output.
Keep its metadata outside agent-writable paths.
Let the host read from a saved sequence after a disconnect.
Record a gap if output is unavailable or exceeds the retained limit.
Do not claim a lossless transcript when a gap exists.

The MVP replay is a readable event history with terminal text and evidence links.
It is not terminal video or a second live TUI.
Sanitize terminal control sequences before replay or Markdown export.
Do not replay raw escape sequences through a host shell or terminal.

Protect local state with owner-only directory and file permissions.
Use existing secret-redaction helpers for recorded content.
Do not store raw operator keystrokes in password-entry modes.
Record the intervention and a protected prompt reference when appropriate.
Redaction is not proof that arbitrary output contains no secrets.

Default export includes the plan summary, candidate IDs, findings, test results, backup state, and operator decisions.
Exclude credential material and raw secret-bearing state.
Require explicit content selection for sensitive terminal artifacts.
Keep raw terminal recording disabled unless the existing secure storage path supports it.
General telemetry contains only approved bounded metadata, not prompts or terminal content.

## Build instructions

1. Add typed methods to the injected host workflow service.
2. Add thin CLI handlers and stable JSON output.
3. Add the small Console view and exact-session actions.
4. Save bounded events and terminal text references.
5. Add the readable Markdown export.
6. Keep manual instance behavior unchanged.

## Local verification

Run K6 with one CLI response group and one Console view test.
Use K7 to prove that the UI opens the same live implementer session.
Check that export contains no credential envelope and that observation gaps remain visible.

## Dependencies

C5 supplies authoritative run state. C2 owns session input and attachment.
C4 supplies checkpoint and preservation records. C6 supplies findings and evidence.
