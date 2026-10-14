# Jackin Workflows — research and implementation specification

**Pack version:** 1.0 — proposed build contract  
**Research date:** 14 October 2026  
**Jackin source baseline:** `453bf611fd974ff88f18caa97aa225af1f8edb2c`  
**Product scope:** the complete supplied Jackin Workflows v1 specification

This pack defines what to reuse, what to build, how components communicate, how failures are handled and what tests establish completion. Product APIs and commands are labeled as proposed. The repository audit is source-backed; no Jackin build, container/VM workload, Pkl fixture or real-agent certification test was executed during this research. No repository changes were made.

## Start here

| Reading order | Document | What it answers |
|---|---|---|
| 1 | [Current-state audit](research/01-current-state.md) | What Jackin already has and which assumptions need correcting |
| 2 | [Gap register](research/02-gap-register.md) | The 25 identified gaps, priorities, owners and closure proof |
| 3 | [System boundaries](architecture/01-system-boundaries.md) | Host/worker trust, crate ownership and non-negotiable invariants |
| 4 | [Domain contracts](architecture/02-domain-contracts.md) and [state/recovery](architecture/03-state-and-recovery.md) | Exact identities, outputs, transitions and crash behavior |
| 5 | [Build sequence](implementation/01-build-sequence.md) and [40-task backlog](implementation/02-task-backlog.md) | Dependency-ordered implementation instructions |
| 6 | [Acceptance matrix](implementation/03-acceptance-and-test-matrix.md) | Proof required for all fourteen product acceptance criteria |

## Main research conclusions

The implementation should reuse existing programmatic launch, runtime/isolation services, Capsule PTYs/control, agent registry, credential boundaries and Docker profile helpers. The current registry has twelve built-ins, including Muse; workflow support must not be reduced to a four-agent assumption. The new layer is durable orchestration and its enforcement/evidence contracts. See the [source-backed audit](research/01-current-state.md) and [evidence register](research/04-evidence-register.md).

Three particularly important distinctions shape the design: existing Docker grants generally widen profile capabilities, so workflow ceilings need their own final authorization boundary; Capsule PTY-writer acknowledgement is not agent processing or exactly-once input; and the project-standard embedded DB is `turso`, not an arbitrary new SQLite binding. Immutable checkpoints, finalization fences, independently trusted tests/reviews and scoped publication gates close the remaining control gaps.

## Pack contents and authority

There are **22 component contracts**, **12 execution-block contracts** and **13 adapter contracts** covering all twelve registered built-ins plus the approved generic CLI path. The supplied Parallel / Join category is split into separate Parallel and Join execution kinds. The original product specification, research, architecture, implementation tasks, examples and export checks are separate Markdown files.

[Source product specification](00-product-specification.md) preserves user scope. Research reports current evidence. Architecture/components define proposed normative behavior; block and adapter files specialize it. Examples are explicitly labeled fixtures, not current executable product support. A certification gate marked unverified is not permission to advertise that capability as implemented.

## Architecture and research

| Document | Purpose |
|---|---|
| [External compatibility](research/03-external-compatibility.md) | pklr, actual agent transports, backends, Git, storage and editor limits |
| [Evidence register](research/04-evidence-register.md) | 37 source entries with pinned repository links and inspected ranges |
| [System boundaries](architecture/01-system-boundaries.md) | Trust model, invariants, scope and crate direction |
| [Domain/wire contracts](architecture/02-domain-contracts.md) | Types, identities, hashes, ports, errors and records |
| [State/recovery](architecture/03-state-and-recovery.md) | Run/node/command transitions, durable effects and crash matrix |
| [Decisions/certification](architecture/04-decisions-and-certification.md) | Selected defaults and release-blocking proofs |

## Component specifications

Each component describes purpose/ownership, current foundation and gap, API/data contract, functionality, security/failure behavior, build steps, dependencies and acceptance tests.

| Component | Specification |
|---|---|
| C01 | [Typed workflow language and Pkl schema](components/01-pkl-schema.md) |
| C02 | [Configuration compiler and immutable plan](components/02-plan-compiler.md) |
| C03 | [Host authorization and capability ceilings](components/03-host-authorization.md) |
| C04 | [Runtime provisioning and sandbox enforcement](components/04-runtime-sandbox.md) |
| C05 | [Agent execution adapters and capability registry](components/05-agent-adapters.md) |
| C06 | [Capsule session control and prompt delivery](components/06-capsule-session-control.md) |
| C07 | [Input leases, observation and takeover](components/07-input-leases-attachment.md) |
| C08 | [Repository isolation and immutable checkpoints](components/08-repository-checkpoints.md) |
| C09 | [Scheduler, dependencies and resource budgets](components/09-scheduler-budgeting.md) |
| C10 | [Durable storage, event ledger and effect outbox](components/10-durable-storage.md) |
| C11 | [Context preparation and artifact storage](components/11-context-artifacts.md) |
| C12 | [Deterministic tests and verification evidence](components/12-tests-evidence.md) |
| C13 | [Independent review and structured findings](components/13-independent-review.md) |
| C14 | [Same-session feedback and bounded repair](components/14-feedback-repair.md) |
| C15 | [Acceptance and operator approval gates](components/15-acceptance-gates.md) |
| C16 | [Checkpoint publication and final GitHub publication](components/16-publication.md) |
| C17 | [Recovery, reconciliation and preservation](components/17-recovery-preservation.md) |
| C18 | [Versioned host workflow service](components/18-host-workflow-api.md) |
| C19 | [Workflow CLI](components/19-workflow-cli.md) |
| C20 | [Console workflow inspection](components/20-console-inspection.md) |
| C21 | [Visual workflow authoring and Pkl preservation](components/21-visual-authoring.md) |
| C22 | [Replay, observability, retention and export](components/22-replay-observability-export.md) |

## Execution-block contracts

| Block | Specification |
|---|---|
| B01 | [Agent block](blocks/01-agent.md) |
| B02 | [Context block](blocks/02-context.md) |
| B03 | [Command block](blocks/03-command.md) |
| B04 | [Test block](blocks/04-test.md) |
| B05 | [Review block](blocks/05-review.md) |
| B06 | [Condition block](blocks/06-condition.md) |
| B07 | [Parallel block](blocks/07-parallel.md) |
| B08 | [Join block](blocks/08-join.md) |
| B09 | [Feedback block](blocks/09-feedback.md) |
| B10 | [Gate block](blocks/10-gate.md) |
| B11 | [Git block](blocks/11-git.md) |
| B12 | [Publish block](blocks/12-publish.md) |

## Agent adapter contracts

| Adapter | Specification |
|---|---|
| A01 | [Claude Code workflow adapter](adapters/01-claude.md) |
| A02 | [Codex workflow adapter](adapters/02-codex.md) |
| A03 | [OpenCode workflow adapter](adapters/03-opencode.md) |
| A04 | [Muse Code workflow adapter](adapters/04-muse.md) |
| A05 | [Approved generic CLI / PTY adapter](adapters/05-generic-pty.md) |
| A06 | [Amp workflow adapter](adapters/06-amp.md) |
| A07 | [Kimi workflow adapter](adapters/07-kimi.md) |
| A08 | [Grok workflow adapter](adapters/08-grok.md) |
| A09 | [Antigravity workflow adapter](adapters/09-antigravity.md) |
| A10 | [Gemini workflow adapter](adapters/10-gemini.md) |
| A11 | [Cursor workflow adapter](adapters/11-cursor.md) |
| A12 | [OMP workflow adapter](adapters/12-omp.md) |
| A13 | [Hermes workflow adapter](adapters/13-hermes.md) |

## Implementation and examples

| Document | Purpose |
|---|---|
| [Build sequence](implementation/01-build-sequence.md) | Ten dependency-ordered stages, crate layout, exact project test commands |
| [Task backlog](implementation/02-task-backlog.md) | Forty assignable work packages with dependencies and required proof |
| [Acceptance/test matrix](implementation/03-acceptance-and-test-matrix.md) | All fourteen criteria, adversarial scenarios and certification evidence |
| [Operator runbook](implementation/04-operator-runbook.md) | Target use, takeover, ambiguity, failures, approval and preservation |
| [Pkl fixtures and workflow](examples/01-pkl-fixtures-and-workflow.md) | Typed language fixtures, exact inputs and full workflow composition |
| [API and recovery examples](examples/02-api-and-recovery.md) | Concrete proposed request/response, unknown-delivery and lease payloads |
| [End-to-end walkthrough](examples/03-end-to-end-walkthrough.md) | Implementation → failure → same-session repair → finalization → publication |

## Export and validation

All substantive content is Markdown. The ZIP preserves this directory structure and relative links; open this README as the entry point. The separately supplied combined Markdown export embeds every document and rewrites local links to internal anchors. It is a reading/export copy, while these separate files remain the easiest implementation handoff units.

[Documentation validation](DOCUMENTATION-VALIDATION.md) records file/link/fence/schema-example checks and their limits. [SHA-256 manifest](MANIFEST.md) lists the exact separate-file digests; it excludes itself to avoid a circular hash. A Markdown renderer may support Mermaid; the text contracts do not depend on diagrams rendering.

Product tests and conformance scenarios are **not run** in this research. Documentation structural checks do not prove runtime security or correctness. The final release must close the explicit certification gates and preserve existing manual Jackin functionality.
