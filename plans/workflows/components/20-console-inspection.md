# C20 — Console workflow inspection

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Make workflows visible and operable in Jackin Console without building a second orchestration state. Implement terminal-native run lists, dependency views, node details and exact-session attachment in the existing `jackin-console` UI.

## 2. Existing foundation and gap

The existing workspace already has the Console crate and runtime attachment APIs. Prior research calls for visible, interruptible sessions, but does not establish a completed workflow graph interface. Reuse current UI/navigation/rendering patterns and original-session attachment. [J02](#evidence-and-references), [J04](#evidence-and-references), [J05](#evidence-and-references).

## 3. View/API contract

Consume `runs.list/get/graph/events`, gate decisions and session-attach methods from C18. A graph node view includes node type, role/agent/provider/model, typed input/output ports, dependency state, attachment names/digests, requested/effective sandbox capabilities, iteration/attempt, session address, current and reviewed checkpoints, findings, artifacts and failure/retention state.

```text
WorkflowViewModel {
  run_snapshot, snapshot_seq, graph_projection,
  selected_node, expanded_scope_iterations,
  active_filters, pending_operations, connection_state
}
```

The model is a cache of host facts. Pending UI operations are distinct from confirmed state. On reconnect, fetch a coherent snapshot/cursor and replay subsequent events; show gaps/connection loss rather than continuing with falsely current status.

## 4. Required functionality

Provide a run list and graph/list hybrid suitable for a terminal. Large graphs support filtering, search, collapsed parallel/repair scopes, keyboard traversal and an expanded execution-attempt view. Source graph and expanded iterations are distinguishable. Color is supplementary; text/icons communicate state accessibly.

Selecting an Agent or Review node reveals the exact logical/physical session and an Observe action. Opening it uses Capsule’s original CLI session, not a Console-rendered transcript substitute. Take control requests the exclusive lease and shows active turn, owner, expiry and paused automation. Replay is a separate action when the session is historical or dead.

Verification panels show which exact revision was tested/reviewed and whether findings are stale relative to the current candidate. Gate panels explain unmet criteria and require an exact-checkpoint decision. Retained-resource panels expose unfinished work, backup status and allowed recovery/export actions. Publication panels distinguish backup, accepted code, pushed branch and PR state.

Pause/resume/cancel/retry/approve operations show their actual pending/confirmed progress. A pause request cannot instantly display “all workers stopped” unless the host reports that boundary. Do not add a local “force success,” “skip verification” or destructive cleanup shortcut.

## 5. Failure and safety

Untrusted labels, paths, findings and agent output are escaped before rendering. Preserve terminal responsiveness under event floods with bounded projections and artifact-on-demand loading. Display no raw credential values. Local cached views must not authorize an action; every mutation carries expected versions to the host.

## 6. Build instructions

Add typed view-model fixtures and terminal snapshots using existing Console testing conventions. Build read-only list/details first, then graph navigation and event reconciliation. Add Observe attachment, then fenced takeover and decisions. Finish retained-work/replay/export panels and keyboard/accessibility tests. Validate graph navigation on small and large fixture sets rather than relying only on one demo DAG.

## 7. Dependencies and integration

C18 service, C07 attach modes, C15 decision reasons, C22 replay/artifacts. No container or database mutation logic lives in the UI. Native desktop additions may consume the same view model later but are not required to duplicate terminal behavior.

## 8. Acceptance tests

`CONSOLE-01`: every required node field is inspectable. `CONSOLE-02`: selected agent/reviewer opens the exact original TUI. `CONSOLE-03`: stale revision and stale connection remain visible. `CONSOLE-04`: control/approval conflicts refresh rather than overwrite. `CONSOLE-05`: graph filtering/navigation works without color. `CONSOLE-06`: manual Console/launch/Capsule interactions retain existing snapshots and e2e behavior.

## Evidence and references

- **J02 — [Cargo workspace](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/Cargo.toml).** Workspace members and package metadata inspected. Version 0.6.4; layered apps/core/services/adapters/ui directories; no workflow crates in the inspected member list.
- **J04 — [Existing orchestration design constraints](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/docs/content/research/agents/orchestration/workflow-systems/agent-workflow-orchestration/02-design-constraints.mdx).** Full document. Proposed deterministic ledger, evidence, operator intervention and Capsule ownership.
- **J05 — [Runtime public integration surface](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime/src/lib.rs).** Full file. Existing launch, inspect, spawn, attach and cleanup re-exports; reactive daemon feature boundary.
- **J19 — [Testing and CI rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/TESTING.md).** Lines 1–200. Pinned mise/mbx/nextest commands, e2e and conditional OrbStack lane, documentation examples and crate gates. Examples here are not evidence of execution.
