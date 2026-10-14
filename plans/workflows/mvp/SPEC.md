# Specification: Jackin Workflows MVP

## 1. Purpose

Build a useful local workflow, not a general agent platform.
The workflow must implement one task, check the result, and repair failures.
The operator must retain access to the original live agent session.
The host must control permissions, checkpoints, and acceptance.

This document defines the first release. It does not declare the full version 1 specification complete.

## 2. Release scope

| Item | MVP rule |
|---|---|
| Host | One local Jackin host daemon. One active workflow run. |
| Backend | One tested local Docker profile. No remote Docker daemon. |
| Platform | Use the available supported development host. Record its OS, architecture, Docker version, and filesystem. |
| Agents | Claude Code implements. Codex reviews in a separate instance. |
| Authentication | One real, selected authentication profile for each agent. Test subscription access when selected. |
| Repository | One exact base revision. Separate private Git state for each mutable worker. |
| Graph | One fixed typed repair sequence. No arbitrary graph language. |
| Parallel work | The implementer can remain alive while one test or review worker runs. No parallel implementation branches. |
| Repairs | One initial turn and at most two repair turns. The limit is part of the plan. |
| Tests | One pinned command runs the trusted suite in a separate worker. |
| Review | One independent reviewer per checkpoint. No review quorum. |
| Storage | One local database and a protected artifact directory. |
| Interface | Workflow CLI, a small Console entry, a fixed stage view, and a readable event history. |
| Backup | Local checkpoints are required. Remote checkpoint backup is an explicit host-authorized option. |
| Acceptance | Automatic only when the latest checkpoint passes the required tests and review. No automatic merge. |

The release report must name the tested combination.
Do not claim support for all hosts, agents, models, or authentication methods.
Keep at most two workflow instances alive at one time.
A paused run keeps its active-run slot.

## 3. Required sequence

```text
Read Pkl
  -> Check inputs and permissions
  -> Save the immutable plan
  -> Prepare a private implementation clone
  -> Start the real Claude Code session
  -> Submit the task
  -> Wait for the end of the active turn
  -> Capture a checkpoint
  -> Save local backup and attempt authorized remote backup
  -> Run trusted tests in a separate worker
  -> Run Codex review in a separate worker
  -> Decide
       pass: check the current source again, then accept
       code failure: send findings to the same Claude session, then repeat
       limit reached: fail and keep work
       uncertain state: wait for the operator and keep work
```

Tests and review run in sequence. Both refer to the same checkpoint.
Do not let the implementer start another automatic turn during these checks.
Run review after a code-test failure to obtain actionable findings.
Stop verification on an infrastructure failure that prevents a valid result.

## 4. MVP requirements

| ID | Required result | Owner | Check |
|---|---|---|---|
| M1 | Typed Pkl produces one immutable plan. Later source edits do not change the run. | C1 | K3 |
| M2 | Real Claude and Codex work with the selected credentials inside restricted instances. | C2, C3 | K1, K7 |
| M3 | Two automatic prompts reach the same live implementer session. | C2, C6 | K1, K7 |
| M4 | Workers cannot write the host repository, task references, or another worker's repository. | C3, C4 | K2 |
| M5 | Tests and review use an exact checkpoint that the implementer cannot change. | C4, C6 | K2, K5, K7 |
| M6 | Repair is bounded. Acceptance uses the latest candidate and valid evidence. | C5, C6 | K5, K7 |
| M7 | Observe does not take input control. Takeover grants one exclusive input owner. | C2, C7 | K1, K7 |
| M8 | A host daemon restart preserves state and does not duplicate uncertain prompts. | C2, C5 | K4, K7 |
| M9 | Checkpoints and incomplete work survive cancellation, failure, and normal cleanup. | C4, C5 | K2, K4, K7 |
| M10 | Backup uses host credentials and exact refs. Backup does not accept or merge code. | C4 | K2, K7 |
| M11 | CLI and Console show the current stage, findings, backup state, and original session. | C7 | K6, K7 |
| M12 | Event history identifies evidence, missing observations, and operator actions. Manual behavior remains intact. | C2, C7 | K6, K7 |

A check ID names a small check group in [CHECKS.md](CHECKS.md).
Each build step runs only its relevant group.
Do not create a separate test suite for each table row.

## 5. Minimum security rules

The workflow requests access. The host grants access.
Apply a final restrictive policy after role and workspace defaults.
Do not use the existing grant merger as the final workflow permission limit.

Give each mutable worker its own repository and instance.
Do not mount the host home, host Git directory, Docker socket, or publisher credentials into a worker.
Task files and reference files must be read-only.
The implementation checkout is writable. Runtime scratch space is private and bounded where the backend supports a hard limit.

Disable DinD, privileged mode, sudo, host namespaces, and extra host capabilities for workflow workers.
Enforce CPU, memory, process, and execution-time limits.
Use a tested network profile for the selected agent endpoints.
Deny unrelated destinations and unnecessary protocols.
Do not enable open networking to fix authentication.

Record the exact network enforcement level.
An IP and port rule is not a hostname or URL rule.
Reject a stronger requested rule when the profile cannot enforce it.

The MVP monitors disk use and stops new work at its configured limit.
This monitor is not a hard filesystem quota.
Reject a requested hard disk quota unless the selected backend already provides a tested quota.
A new quota subsystem is outside the MVP. This is an explicit limit of the first release.

## 6. Vendor subagents

Make delegation an explicit choice in the Pkl input.
The MVP permits local vendor subagents within the worker's enforced limits.
They share the worker's trust boundary. They are not independent reviewers.
A process limit is not a limit on logical model subagents or tokens.
Do not advertise a strict token-cost limit without provider support.

Support a `forbid` choice only when the adapter can enforce it.
Reject that choice when enforcement is unknown.
Do not permit separate remote agent jobs in the MVP.
Disable any exposed remote-job tool. Reject an adapter mode that requires such jobs.
Record visible child relationships. Mark unavailable relationships as unknown.
Do not invent a complete lineage from terminal text.

## 7. Checkpoint rule

Use a brief whole-container pause for the first Docker implementation.
The pause must not terminate the original agent or Capsule processes.
Live output stops during the pause. The interface must show this state.
A short frozen interval does not violate the original-session requirement.

Copy the source into a host-owned staging directory while worker writes are blocked.
Create the checkpoint in a separate trusted Git repository.
Do not run worker-controlled Git hooks or configuration on the host.
Resume the same processes after the checkpoint is safe.

Check the source again before acceptance.
A changed source tree invalidates the previous acceptance decision.
An operator write also invalidates pending acceptance.
See [C4](components/04-checkpoints-and-backup.md) for the exact capture procedure.

## 8. Recovery rule

Save intent before a prompt, instance launch, checkpoint operation, or remote push.
Keep state changes and their event records in one database transaction.
Use stable operation IDs.

A PTY receipt means that bytes reached the PTY writer.
It does not prove that the agent processed the prompt.
When delivery is uncertain, stop automatic input.
Do not resend that prompt automatically.
The operator can inspect the original session and provide the next explicit instruction.

Recover from a host daemon restart when the original worker session remains alive.
Do not claim that a dead agent process is the same live session.
Preserve its repository and report `session_lost` instead.

## 9. Product acceptance

Accept only a checkpoint with passing trusted tests and a valid passing review.
The evidence must name the candidate ID, commit, tree, and test-policy hash.
The host must confirm that no newer source changes require another check.
The host must hold automatic input ownership at this decision.

Remote backup success is separate from acceptance.
For `backup.mode = required`, incomplete backup blocks further workflow progress.
For `backup.mode = best_effort`, show a warning and keep the local checkpoint.
For `backup.mode = off`, record that the operator chose local-only work.

Keep incomplete and unpushed work until an explicit operator action permits its removal.
Do not add automatic destructive cleanup to this MVP.

## 10. Deferred scope

Do not build a general DAG scheduler, dynamic branches, or parallel implementers.
Do not build a visual authoring interface or a web service.
Do not add Muse or generic PTY workflow support before the core MVP passes.
Do not add PostgreSQL, distributed scheduling, microVM providers, or immutable object-cache optimization.
Do not build PR creation, merge, broad approval gates, or automatic agent-process replacement.
Do not build full terminal video replay, complete tool tracing, or a provider-wide certification matrix.
Do not add Rig, Nanocode, tmux, or a new PTY owner.

These limits reduce implementation work. They do not permit unsafe claims about unsupported capabilities.

## 11. Relationship to the original acceptance criteria

| Original criterion | MVP coverage |
|---|---|
| AC1 | Typed fixed-template Pkl and an immutable plan. General graphs follow later. |
| AC2 | Two tested real CLI agents, not every adapter. |
| AC3 | Exact inputs and stated enforced permissions. Hard disk quotas remain conditional. |
| AC4 | Independent writable worker state. Parallel implementation scheduling follows later. |
| AC5 | Independent tests and review against exact checkpoints. |
| AC6 | Automatic feedback to the original healthy session. |
| AC7 | Bounded repairs and a latest-candidate acceptance check. |
| AC8 | Original live TUI access, with a declared short checkpoint pause. |
| AC9 | Exclusive and recorded operator control. |
| AC10 | Local durable state and host-restart recovery. Agent-process replacement follows later. |
| AC11 | No silent loss of incomplete or unpushed work. |
| AC12 | Host-only checkpoint backup. PR and merge operations remain disabled. |
| AC13 | Fixed graph inspection and Console access. Visual authoring follows later. |
| AC14 | Documented manual behavior remains intact. Internal API compatibility is not required. |

## 12. Sources and terms

See [REVIEW.md](REVIEW.md) for review decisions.
See [SOURCES.md](SOURCES.md) for source limits and current repository findings.
Use the terms in [TERMS.md](TERMS.md).
