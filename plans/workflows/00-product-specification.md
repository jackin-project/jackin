# Source product specification

**Source:** Product specification supplied by the user in this conversation. This document preserves the requested product scope. The other files distinguish current implementation evidence from proposed build contracts and clarify unresolved execution semantics.

---

# Jackin Workflows
## Final Product Specification — Version 1.0

**Status:** Proposed final architecture  
**Implementation:** Rust, integrated into Jackin  
**Configuration:** Apple Pkl, evaluated by `pklr`  
**Execution:** Existing AI coding-agent CLI applications  
**Isolation:** Jackin-managed containers and virtual machines

---

## 1. Product vision

Jackin Workflows is a Rust-native, durable workflow orchestration system that uses real AI coding-agent CLI applications as isolated, reusable execution blocks.

The system must replace unrestricted, prompt-driven coordination with **deterministic execution controlled by a host workflow engine**.

AI agents remain responsible for implementing software, analyzing code, researching problems, and reviewing changes.

Jackin is responsible for controlling what agents can access, when they execute, how they interact, how their work is verified, and whether their outputs are accepted.

**Core principle: Deterministic workflows, constrained AI workers, independently verified results.**

The system must not recreate Claude Code, Codex, Muse Code, OpenCode, or other coding-agent applications. It must use their existing runtimes.

## 2. Fundamental architecture

The system has six principal layers.

| Layer | Technology | Responsibility |
|---|---|---|
| Workflow definition | Apple Pkl | Declare tasks, permissions, dependencies, and acceptance criteria |
| Configuration compiler | `pklr` + Rust | Evaluate configuration, validate contracts, produce an immutable plan |
| Workflow orchestrator | Rust + Tokio | Scheduling, state transitions, retries, and coordination |
| Execution platform | Jackin host runtime | Provision isolated role instances and manage their lifecycles |
| Session platform | Jackin Capsule | Start, control, observe, and attach to actual CLI agents |
| Verification | Rust + agent reviewers | Tests, reviews, feedback, and publication gates |

The orchestrator runs **on the host**, integrated with Jackin's existing host daemon and execution services.

The coding agents run inside Jackin-managed isolated environments.

Jackin Capsule owns the real CLI process, PTY, session state, terminal rendering, and input routing.

The workflow engine never needs to recreate an agent's terminal interface.

## 3. Workflow definitions

Workflows must be declared using Apple Pkl.

Each workflow consists of reusable, typed execution blocks.

Supported final block categories:

| Block | Purpose |
|---|---|
| Agent | Start a real coding-agent application and execute instructions |
| Context | Prepare reference documents, files, and task inputs |
| Command | Run a deterministic command in a sandbox |
| Test | Execute automated verification |
| Review | Independently inspect a specific code revision |
| Condition | Select a path based on structured evidence |
| Parallel / Join | Execute independent tasks concurrently and synchronize results |
| Feedback | Return findings to an existing agent session |
| Gate | Enforce acceptance criteria or require operator approval |
| Git | Checkpoint, commit, compare, and manage branches |
| Publish | Push authorized changes, create PRs, or produce artifacts |

Each block must declare its identity, input data, output contract, permissions, dependencies, execution limits, and completion criteria.

### Configuration requirements

Pkl must support reusable templates, inheritance, parameterization, shared sandbox profiles, typed constraints, and versioned imports.

The Rust compiler must verify graph correctness, dependencies, resource restrictions, authorization, model availability, and agent compatibility before execution.

The evaluated configuration must become an **immutable execution plan**, identified by a cryptographic hash.

Changes to a Pkl file during an active run must not silently change that run.

Existing Jackin role manifests and workspace configuration remain compatible. Workflow definitions reference these existing resources rather than replace them.

## 4. Agent execution model

Every agent block must use an actual coding-agent CLI.

Jackin's existing supported runtimes form the initial foundation. Additional runtimes, including Muse Code, must be integrated through dedicated adapters.

The architecture must also support arbitrary compatible CLI applications through a generic PTY adapter.

### Session requirements

Every agent execution must support, where the application permits:

- Programmatic startup and initial prompt submission.
- Long or short instructions, including supported `/goal` commands.
- Attached task documents and reference materials.
- A persistent, identifiable CLI session.
- Streaming execution status and output.
- Follow-up prompts delivered to the same session.
- Cancellation, interruption, and session recovery.
- Live operator attachment to the original TUI.

Agent adapters must explicitly report unsupported capabilities. The system must not invent session-resume or steering capabilities.

The workflow engine must not use terminal readiness or an agent's self-reported completion as proof of successful implementation.

### Relationship to Jackin roles

A Jackin role is a reusable execution environment, **not a workflow block**.

The same role can be used by multiple blocks with different agents, tasks, repositories, and resource limits.

Each concurrently writable agent block must normally receive its own isolated Jackin instance.

Multiple agents in the same Capsule instance share the same filesystem and namespaces; that arrangement must be explicit rather than treated as independent isolation.

## 5. Sandbox and security requirements

Each worker must execute inside a restricted Jackin-managed environment.

The workflow definition requests capabilities, but the host's authorization policy determines what is actually permitted.

The system must enforce:

- Explicit repository mounts and writable directories.
- Read-only task references and attachments.
- CPU, memory, process, disk, and execution-time limits.
- Approved network destinations and protocols.
- Scoped credentials and tool capabilities.
- Isolation from other workers and host resources.
- Explicit permissions for Git and GitHub mutations.

The host must reject a workflow block if its required isolation guarantees cannot be enforced.

Docker is the initial common backend. Jackin's Apple Container backend can be used where required capabilities are supported. Additional sandbox or KVM-backed microVM providers must fit behind a common Rust interface.

**Docker-in-Docker must not bypass workflow security guarantees.** Strictly restricted workflows must disable DinD unless equivalent enforcement is implemented for inner containers.

Private Git clones should be the default for isolated parallel execution. Git worktrees may be used when their shared-reference behavior is explicitly acceptable.

No agent may expand its own permissions by editing its prompt, role configuration, or workflow definition.

## 6. Deterministic execution and verification

The orchestrator must execute a defined state machine.

The standard implementation workflow is:

1. Validate workflow configuration and resolve all inputs.
2. Prepare an isolated repository at an exact Git revision.
3. Provision the Jackin role instance and start the coding agent.
4. Deliver task instructions into the agent session.
5. Capture an immutable code checkpoint.
6. Run independent tests and reviews against that checkpoint.
7. Evaluate structured results against acceptance criteria.
8. Return actionable failures to the original agent session.
9. Repeat the repair-and-verification cycle within configured limits.
10. Accept, fail, or request operator approval based on evidence.

The main workflow graph must be acyclic. Repair and feedback relationships are explicitly bounded loops, not uncontrolled graph cycles.

### Independent verification

Test workers and review agents must execute independently of the implementer.

Reviewers must receive the **exact immutable revision** being evaluated, not a mutable copy of the implementation agent's working directory.

Reviews may run concurrently with ongoing implementation, provided every finding is associated with the correct checkpoint.

Findings must be structured, traceable, and actionable.

A finding against an outdated revision cannot be blindly applied to a newer one.

### Feedback loop

When verification fails, the orchestrator sends a follow-up prompt to the implementation agent's existing session.

The feedback must contain the relevant failures and references to their evidence.

The session must not be restarted merely to deliver review findings when same-session continuation is supported.

The orchestrator must coordinate feedback with the agent's active turn and any operator intervention.

An agent cannot decide to skip verification or declare its own work accepted.

## 7. Live TUI access and debugging

**Every interactive coding-agent workflow block must remain inspectable through Jackin Capsule.**

The user must be able to connect to the original running CLI from Jackin Console or the host terminal.

Jackin's native Capsule multiplexer is the terminal authority. No additional tmux layer is required.

The final system must support three operator modes.

**Observe:** Attach to the running CLI without taking keyboard ownership or interrupting execution.

**Take control:** Explicitly acquire an exclusive input lease, pause conflicting automated input, interact directly with the CLI, and return control to the scheduler.

**Replay:** Inspect previous terminal output, agent events, submitted prompts, verification results, Git changes, and operator interventions after execution.

Each session must have stable identity and event correlation.

Debug artifacts must be retained according to a configured security and retention policy, with sensitive information protected.

A failed sandbox should be preservable for investigation without preventing durable workflow failure reporting.

## 8. Durable workflow execution

Workflow state belongs to the Jackin host, not to an agent's conversation.

The host must persist workflow runs, node attempts, exact Git revisions, instance identities, session identities, prompts, outputs, review findings, verification evidence, retry counts, operator decisions, and publication records.

The engine must support:

- Concurrent execution of independent graph nodes.
- Pausing, resuming, cancelling, and retrying work.
- Restart recovery and reconciliation with active Jackin instances.
- Idempotent or reconciled external side effects.
- Explicit operator approval gates.
- Resource and concurrency budgets.
- Reliable preservation of incomplete work.

A daemon crash must not result in silent duplicate prompts, lost work, or false success.

SQLite is suitable for the initial local persistence layer. A PostgreSQL backend may support future distributed scheduling.

## 9. Git and publication safety

Each implementation attempt must have an isolated repository state and traceable Git revision.

Checkpoint commits should be created frequently. When authorized, the host should push them promptly to run-specific remote branches so unfinished work does not remain only on one machine.

**Checkpoint publication is separate from acceptance or merge.**

The orchestrator must prevent deletion of the only remaining copy of unpushed work.

Final publication must verify the latest accepted revision, confirm required checks, and obey the configured authorization policy.

GitHub credentials used for publication should normally remain in the host control plane rather than be given to implementation agents.

No implicit force-push, destructive cleanup, or merge into `main` is permitted.

## 10. Visual workflow editor

Jackin must provide a workflow graph representation.

Each node must display its type, dependencies, chosen agent and role, attached documents, sandbox capabilities, execution state, current Git revision, review findings, and artifacts.

The operator must be able to inspect an agent node and open its actual live Capsule session.

The final visual authoring interface must allow creation and configuration of reusable blocks, typed connections, attachments, policies, and acceptance gates.

Pkl remains the authoritative source. The editor must preserve supported hand-authored configuration structures rather than destructively regenerate arbitrary Pkl source.

Jackin Console provides terminal-native workflow inspection and execution control. A richer visual editor may use React Flow or an equivalent graph-editing framework while consuming the same host workflow API.

## 11. Host API and command surface

The host daemon must expose a versioned workflow control API.

Its operations include validating, planning, starting, listing, monitoring, pausing, resuming, cancelling, retrying, approving, attaching, and exporting workflow runs.

The intended CLI namespace is `jackin workflow`.

Example commands include `jackin workflow validate`, `plan`, `run`, `status`, `graph`, `attach`, `events`, `pause`, `resume`, `cancel`, `retry`, `approve`, and `export`.

These are **new target commands**, not commands claimed to be present today.

The CLI, console, and visual interface must consume one authoritative workflow service.

## 12. Jackin implementation boundaries

The preferred crate organization is:

| Crate or existing subsystem | Responsibility |
|---|---|
| New `jackin-workflow-spec` | Pkl evaluation, types, schema validation, graph compilation |
| New `jackin-workflow` | Scheduler, state machine, gates, review loops, run coordination |
| Workflow storage layer | Durable run state, events, attempts, artifacts |
| Existing `jackin-runtime` | Headless instance provisioning and sandbox lifecycle |
| Existing `jackin-isolation` | Pinned clones and Git isolation |
| Existing `jackin-capsule` | Real CLI sessions, PTYs, input, output, live attachment |
| Existing `jackin-protocol` | Typed session and workflow communication |
| Existing host daemon | Workflow supervision and RPC endpoints |
| Existing Jackin Console | Workflow status, graph inspection, attachment, debugging |

The implementation must respect Jackin's existing crate boundaries and avoid duplicating PTY, credential, runtime, image-building, or container-management functionality.

Rig and Nanocode are optional future integrations, not required architectural dependencies.

## 13. Final acceptance criteria

The system is complete when:

1. A typed Pkl workflow compiles into a validated, immutable execution plan.
2. Real supported coding-agent CLIs launch autonomously inside Jackin-managed isolated instances.
3. Agents receive exact instructions, references, resources, and permitted capabilities.
4. Parallel agents cannot interfere with unauthorized repositories or another worker's mutable state.
5. Tests and independent reviews execute against immutable code revisions.
6. Failed verification automatically produces correlated feedback to the original agent session.
7. Repair attempts are bounded and all final acceptance gates evaluate the latest revision.
8. A user can inspect and attach to the original live TUI without terminating the workflow.
9. Manual takeover is exclusive, auditable, and safe to relinquish.
10. Execution states, events, checkpoints, and diagnostic evidence survive failures and restarts.
11. Unaccepted or unpushed work is not silently destroyed.
12. Publication and merge permissions are enforced by the host.
13. The workflow graph is visible and operable from Jackin, with a visual authoring interface.
14. Existing Jackin manual CLI, Console, Capsule, role, and workspace functionality continues working.

---

## Final architectural decision

**Build the workflow orchestrator as a native subsystem of Jackin's host execution model.**

Do not build another agent platform.

Do not replace the real coding-agent CLIs.

Do not duplicate Jackin Capsule or its terminal multiplexer.

Use Pkl to define the execution contract, Rust to control its lifecycle, Jackin to provision and connect to real coding agents, sandbox backends to enforce restrictions, and independent verification to determine whether a task succeeded.

**The result is a controlled, observable, recoverable workflow platform that turns existing AI coding agents into composable execution building blocks.**
