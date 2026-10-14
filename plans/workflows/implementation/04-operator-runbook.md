# Operator runbook and implementation handoff

**Status:** Target operating instructions for the feature to be implemented. The `jackin workflow` commands below are not present at the audited commit. This runbook becomes executable after the corresponding implementation stages and documentation tests pass.

## Before starting a run

Select trusted existing role/workspace references, the exact repository starting revision, certified agent/provider/model, explicit network/credential scope and finite budgets. Put task/reference documents into the approved context bundle. Decide whether checkpoint backup is allowed, whether publication is local-only or GitHub, and which exact approval is required. Do not provide GitHub publishing credentials to the implementation agent.

Validate and plan. Read effective permissions, unsupported capability diagnostics, role/image/binary pins, graph/repair limits and final gates. Start by stored plan hash. Later edits to the source definition create a new plan, not a silent update to the active run.

```sh
# Target command examples; replace identifiers with real host-returned values.
jackin workflow validate ./workflow.pkl
jackin workflow plan ./workflow.pkl --format json
jackin workflow run --plan <plan-hash>
jackin workflow status <run-id>
jackin workflow events <run-id> --follow
```

## Inspecting and taking control

Use Observe to inspect the original running CLI without stopping it. Take control only when you intend to acquire exclusive input authority. Check the active turn; taking control may require a separate certified interrupt. Release control through the explicit workflow/Capsule action and review any reconciliation/verification requirement before resuming.

```sh
jackin workflow attach <run-id> --node implement --observe
jackin workflow attach <run-id> --node implement --control
jackin workflow pause <run-id> --mode drain
jackin workflow resume <run-id>
```

Closing an observing terminal does not cancel the run. Disconnecting a controlling terminal does not silently authorize automation to resume. Manual source edits invalidate previous acceptance eligibility and require a fresh checkpoint/verification.

## When input delivery is unknown

Stop automated input. Inspect the pending command, exact payload digest, Capsule/vendor receipt and current session. Do not rerun the previous command because the network timed out. The host reconciliation flow may prove receipt or absence; otherwise an authorized operator records one of: observed received, explicit permission to send a replacement despite uncertainty, or abandon. A replacement uses a new command ID linked to the original and remains auditable. An operator attestation is not relabeled as a vendor acknowledgement.

## When tests or reviews fail

Read findings for the exact checkpoint, not just the current branch label. The repair controller sends one correlated failure bundle to the original healthy implementer session when its input boundary and lease permit. Observe the new checkpoint and independent verification. Repair exhaustion is a deliberate final/operator state; it is not fixed by resetting counters or skipping gates.

## Approval and publication

Review the exact latest candidate, required tests/reviews, changed files and any requested waiver. Approve only the named checkpoint/action. A stale approval will be rejected if code/evidence changed. Checkpoint backup is not merge or acceptance. Final PR publication uses the host’s authorized action, not a prompt instructing the agent to run GitHub commands.

```sh
jackin workflow approve <run-id> --gate publish-approval --checkpoint <checkpoint-id>
jackin workflow status <run-id> --format json
jackin workflow export <run-id> --format markdown
```

## Failure, cancellation and preserved work

Cancel to stop further work while retaining unfinished state. Inspect retained resources, unpushed checkpoints, untracked-state archives and publication reconciliation. A failed run can legitimately have a preserved sandbox. Do not bypass preservation holds with ordinary cleanup or force flags. Quarantined diagnostic access is separately authorized and does not restore old credentials automatically.

```sh
jackin workflow cancel <run-id> --reason "Operator stopped this attempt"
jackin workflow status <run-id>
jackin workflow export <run-id> --format bundle
```

If storage is full, free only unheld/verified redundant data or export a verified copy through the authorized path. Unique unfinished work must not be deleted to make a workflow look recovered. A lost disk without independent backup is outside a local-only preservation guarantee.

## Implementation team handoff

Start with the README reading order, then [architecture](../architecture/01-system-boundaries.md), the owning component and [task backlog](02-task-backlog.md). Recheck the pinned existing code before implementing. Keep capabilities unverified until tests prove them. Do not publish or merge changes merely because this specification describes a publication component.
