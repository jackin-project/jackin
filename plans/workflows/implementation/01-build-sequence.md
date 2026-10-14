# Dependency-ordered implementation sequence

## Working agreement

This is an implementation plan, not a report that code was built. Begin from a clean or explicitly preserved working tree. Read the repository’s current AGENTS/engineering/testing/dependency rules and compare the implementation base to audit commit `453bf611fd974ff88f18caa97aa225af1f8edb2c`. Reconcile moved APIs before coding. Do not modify existing manual behavior to make a workflow prototype easier.

No calendar estimates are assigned. A stage completes only when its specified artifacts and tests exist. The first vertical slice is intentionally narrower than final v1, but it does not waive security, session identity, preservation or durability.

## Stage 0 — Baseline and proof harness

Record the implementation commit, installed tools, existing runtime/role/backends and dependency rules. Add a workflow feature boundary and baseline compatibility fixtures. Define capability-certificate schemas and test evidence locations. Reproduce the existing project gates before changing behavior; a pre-existing failure is recorded, not hidden.

**Outputs:** updated evidence map, crate dependency diagram, CI/test fixture scaffolding. **Exit:** baseline manual behavior known, research findings reconciled, no unreviewed additional runtime/framework dependency.

## Stage 1 — Domain, language and authorization

Create `crates/core/jackin-workflow-spec` with domain types, versioned schema, diagnostics, graph/port validation and pure policy-request types. Add the pinned pklr evaluator-only helper, frozen import resolver, canonical hash and source bundle. Implement restrictive host authorization independently from legacy widening grant merges.

**Outputs:** valid/invalid Pkl fixtures, plan golden hashes, policy algebra tests, denied-IO fixtures. **Exit:** CERT-PKL and core plan/policy tests; no agent has to run yet.

## Stage 2 — Durable engine against fake ports

Create `jackin-workflow` and `jackin-workflow-store`. Implement turso schema/migrations, state reducer, events/outbox, controller epochs, artifact sealing and resource reservations. Build fake clock/runtime/Capsule/remote services with fault injection. Include prompt-unknown and preservation states from the start.

**Outputs:** deterministic simulations, crash-safe transitions, event cursor queries and minimal host service injection interface. **Exit:** CERT-STORE; no side effect occurs before durable intent. A fake-platform success is not real backend certification.

## Stage 3 — Certified execution and control substrate

Wrap existing headless launch and private-clone materialization. Add final execution grants, workflow ownership labels, exact image/binary pins, backend checks, disk/network enforcement and worker deadlines. Add Capsule epoch identity, command journal, submission receipts, all-ingress input leases and observer behavior. Add consistent checkpoint capture and preservation holds before automatic cleanup is possible.

**Outputs:** real backend conformance suite, original-session observe/control and sealed checkpoints. **Exit:** CERT-SANDBOX, CERT-CAPSULE and basic CERT-REVISION. Do not treat a successful `docker run` or visible TUI as sufficient.

## Stage 4 — First real vertical slice

Select one exact certified interactive agent build using existing runtime installation/auth. Execute a task, capture a checkpoint, run an independent trusted test and an independent real reviewer, deliver one bounded repair to the same healthy session, then require exact-revision human approval. Keep the original TUI inspectable throughout. Inject a host crash during the run and reconcile without duplicate input.

**Outputs:** end-to-end evidence bundle with plan, commands, session identities, checkpoints, tests, review, feedback and decision. **Exit:** correct behavior under success, failure, interruption and uncertain delivery. No final-v1 claim yet.

## Stage 5 — Complete orchestration semantics

Finish all twelve block kinds; add conditions, parallel/join, explicit Git integration and finite repair expansion. Complete quotas, flaky/infra retry separation, stale-review handling and latest-candidate gates. Ensure retained implementers and verifier resource admission cannot deadlock.

**Outputs:** block catalog fixtures and parallel/integration/repair demonstrations. **Exit:** every block has typed inputs/outputs, deterministic completion and negative tests. Mandatory gates cannot be bypassed.

## Stage 6 — Publication and recovery hardening

Implement separate checkpoint backup and accepted publication, scoped host credentials, expected remote refs, PR marker reconciliation, revocation and hold release. Complete quarantine, retained sandbox inspection, backups, migration/storage-full recovery and hold-aware cleanup across existing commands.

**Outputs:** lost-response/conflict/offline publication tests and only-copy protection report. **Exit:** CERT-PUBLISH plus complete recovery matrix. No force/default-branch mutation or silent loss of unpushed work.

## Stage 7 — Full adapter coverage

Enumerate `Agent::ALL`: Claude, Codex, Amp, Kimi, OpenCode, Grok, Antigravity, Gemini, Cursor, Muse, OMP and Hermes. Add capability profiles/workflow integration for every existing runtime and the approved generic CLI path. Certify supported capabilities per exact build and explicitly reject unsupported/unverified ones. Do not alter the sealed built-in runtime trait into an uncontrolled plugin loader.

**Outputs:** capability matrix, per-adapter fixtures, actual CLI evidence. **Exit:** every built-in has an honest supported/unavailable workflow capability row, all advertised modes pass CERT-AGENT, existing manual use remains intact. A product release must state its actual certified workflow support rather than imply every capability works for every agent.

## Stage 8 — Full host API, CLI and Console

Complete C18 methods, compatibility negotiation, local authentication and event replay. Add all `jackin workflow` commands, help, JSON/exit behavior and original-session attach. Add Console graph/list/details, gates, retained work and replay navigation. Thin clients consume the same service.

**Outputs:** protocol fixtures, CLI tests and Console snapshots/e2e. **Exit:** target command surface usable, no duplicated engine, no client-side authority or lease bypass.

## Stage 9 — Visual authoring, export and final acceptance

Implement host source-aware edit/preview/CAS APIs, managed Pkl module mode, certified surgical edits, complete block forms and typed connections. Finish safe replay, classified export, retention and telemetry governance. Run every acceptance criterion and compatibility/platform gate.

**Outputs:** real authoring demo, preservation fixture corpus, complete acceptance matrix and release capability report. **Exit:** all fourteen supplied acceptance criteria pass. A graph viewer without authoring cannot close this stage.

## Dependency-safe composition

The application/host composition root constructs the workflow service, store and runtime adapter and injects them into the daemon lifecycle. The existing runtime umbrella may re-export the daemon shell, so the daemon leaf must not import that umbrella to call itself indirectly. Keep the workflow core dependent on abstract ports, never on `jackin-runtime` or UI.

A host-owned integration module can implement workflow ports using existing runtime services; extract it into a thin leaf crate only when dependency rules require it. It delegates existing functionality rather than copying Docker, credential or PTY code. The daemon accepts an injected workflow-service interface. Verify the actual Cargo graph after each extraction to prevent `runtime → daemon → workflow → runtime` cycles.

## Suggested source layout inside new crates

`jackin-workflow-spec`: `domain/`, `schema/`, `compiler/graph.rs`, `compiler/ports.rs`, `compiler/canonical.rs`, `compiler/diagnostics.rs`, evaluator/resolver boundaries and fixture tests. `jackin-workflow`: `ports/`, `reducer/`, `scheduler/`, `repair/`, `verification/`, `gates/`, `publication/`, `recovery/`. `jackin-workflow-store`: `schema/`, `migrations/`, `transactions/`, `outbox/`, `queries/`, `backup/` and crash tests. Keep file-size and allowed-dependency rules in force; logical components do not need separate crates.

## Required project commands

The following are instructions based on the inspected testing policy, **not commands executed during this research**. Commands naming new crates become valid after those workspace members exist.

```sh
mise install
mise exec -- mbx +1.99.0 nextest run -p jackin-workflow-spec
mise exec -- mbx +1.99.0 nextest run -p jackin-workflow -p jackin-workflow-store
mise exec -- mbx +1.99.0 nextest run -p jackin-capsule -p jackin-console
mise exec -- mbx +1.99.0 clippy -p jackin-workflow-spec -p jackin-workflow --all-targets -- -D warnings
mise exec -- mbx +1.99.0 xtask ci --fast
mise exec -- mbx +1.99.0 xtask ci
mise exec -- mbx +1.99.0 xtask ci --e2e
mise exec -- mbx +1.99.0 xtask ci-doc-examples --package jackin-workflow-spec
mise exec -- mbx +1.99.0 xtask lint files
mise exec -- mbx +1.99.0 xtask lint agents
```

Runtime/container changes require the real Docker-backed lane and current Capsule build preparation documented in TESTING.md. Changes touching the specified usage/launch assembly surfaces also require the repository’s Apple Silicon macOS 27/OrbStack lane; a generic Linux Docker run does not substitute for it. Zero selected e2e tests is failure. New crate README/AGENTS/dependency rules and documentation example tests must be included. Do not hand-edit generated CI workflow outputs instead of their authoritative inputs.

## Evidence and references

- **J02 — [Cargo workspace](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/Cargo.toml).** Workspace members and package metadata inspected. Version 0.6.4; layered apps/core/services/adapters/ui directories; no workflow crates in the inspected member list.
- **J05 — [Runtime public integration surface](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime/src/lib.rs).** Full file. Existing launch, inspect, spawn, attach and cleanup re-exports; reactive daemon feature boundary.
- **J06 — [Programmatic launch options](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-launch-load-options/src/load_options.rs).** Lines 1–300. Non-interactive launch, programmatic constructor, identity sink, inputs and validation error vocabulary. Deeper launch call paths were not executed.
- **J10 — [Host daemon shell](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-host-daemon/src/host_daemon.rs).** Lines 1–260. Protocol version 2; Hello, Status, TelemetryHealth, AttentionSnapshot and Shutdown request variants. Not a workflow RPC service.
- **J18 — [Engineering rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/ENGINEERING.md).** Full document. Rust-first, reuse, turso general-purpose DB, governed jackin-telemetry and privacy rules.
- **J19 — [Testing and CI rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/TESTING.md).** Lines 1–200. Pinned mise/mbx/nextest commands, e2e and conditional OrbStack lane, documentation examples and crate gates. Examples here are not evidence of execution.
- **J20 — [Current CLI root](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/apps/jackin/src/cli.rs).** Lines 1–230, including full top-level Command enum. Workflow namespace absent. CLI parsing separated from handlers.
- **J22 — [Built-in agent identity and dispatch registry](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-core/src/agent.rs).** Lines 1–240. Complete Agent enum, Agent::ALL, canonical slugs, auth modes and runtime dispatch. Twelve registered built-in agents; not a workflow capability certificate.
