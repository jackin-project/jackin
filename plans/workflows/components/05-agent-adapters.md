# C05 — Agent execution adapters and capability registry

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Adapt existing coding-agent CLI behavior to a uniform workflow session contract without implementing a new agent runtime. Maintain a version-certified capability registry in the workflow integration layer and add only reusable metadata extensions to existing agent services.

## 2. Existing foundation and gap

The inspected `AgentRuntime` trait is sealed and handles installation, auth, paths and versions. It is not an open plugin or workflow-turn-control interface. Muse already implements that runtime trait. Preserve both facts: wrap built-ins and extend their existing setup, rather than reimplementing them or claiming arbitrary external implementations can use the sealed trait. [J16](#evidence-and-references), [J17](#evidence-and-references).

## 3. API and data contracts

```text
probe(AgentDescriptor, RuntimeEnvironment) -> CapabilityManifest
prepare_session(SessionStartSpec) -> CapsuleLaunchSpec
prepare_input(SessionBinding, PromptEnvelope) -> InputTransaction
interpret_event(AuthenticatedVendorEvent) -> NormalizedAgentEvent
interrupt(SessionBinding, expected_turn) -> InterruptAction
recover(SessionBinding, VendorObservation) -> RecoveryAssessment
```

Capsule executes the launch/input actions and owns process/PTY lifecycle. The adapter does not create an independent terminal. `CapabilityManifest` is indexed by agent binary digest/version, transport/protocol, mode, provider/model constraints and certificate version. Capabilities separately include initial prompt, long prompt, file references, live TUI, same-session follow-up, active-turn steering, semantic acknowledgement, cancellation, conversation resume, original-session reattach, structured output and approved slash commands.

Each capability has `supported`, `unsupported` or `unverified`, a reason and evidence/certificate reference. “Supported with PTY heuristics” must identify the exact limited synchronization contract; it is not semantic completion proof. A plan declares required capabilities; missing required capability is a compilation/admission error.

## 4. Required functionality

Prefer stable structured events for correlation when the selected mode can still preserve the real original TUI. Otherwise use version-tested PTY input behavior, with no claim of vendor processing acknowledgement. Attachments are mounted read-only; adapters submit exact approved references or attachment formats and record content hashes.

An input transaction separates literal content from submit/interrupt/slash actions. Long instructions use supported file/attachment delivery or bounded paste chunks under one lease; never silently truncate. Record the exact submitted bytes or supported structured payload in protected artifacts. Reject embedded control sequences where literal-text transport cannot represent them safely; escape or require a different certified transport.

`/goal` is an explicitly requested, capability-checked command. It does not change Jackin deadlines, repair scope, authorization or independent acceptance. Agent configuration that enables arbitrary vendor subagents is constrained by the same instance resources and filesystem rights; those subagents do not count as independently isolated reviewers.

Generic compatible CLI support uses a host-approved `ExecutableDescriptor`: immutable executable digest, argv template, role/platform compatibility, declared IO mode, permitted environment and certified capability profile. Do not load arbitrary downloaded executable plugins from Pkl. Existing built-in agent selection remains supported unchanged.

## 5. Failure and recovery

Authentication/availability failure is a recorded adapter/runtime error. The adapter cannot switch accounts/models or broaden permissions unless a fallback was explicit in the plan. A vendor conversation resume creates a new physical session binding when processes changed; it must never reuse a stale Capsule epoch. Same-session feedback during a healthy session must not restart it merely for convenience.

If a requested combination has structured automation but cannot expose the original live interactive session, it does not satisfy the interactive block contract. Reject or require an explicitly different non-interactive block mode; never render logs and label that “the original TUI.”

## 6. Build instructions

Add the capability model and fake adapter before vendor-specific integrations. Wrap the existing built-in registry. Implement one interactive adapter end-to-end through Capsule, then add certified structured transports. Capture reviewed fixtures and real-binary tests for each supported tuple. Provide an operator-readable capability report used consistently by compiler, CLI and UI.

## 7. Dependencies and integration

Depends on C03, C04, C06 and C07. Individual contracts: [Claude](../adapters/01-claude.md), [Codex](../adapters/02-codex.md), [OpenCode](../adapters/03-opencode.md), [Muse](../adapters/04-muse.md), [generic PTY](../adapters/05-generic-pty.md).

## 8. Acceptance tests

`ADP-01`: unsupported resume/steer/goal fails explicitly. `ADP-02`: exact payload/attachments survive long input. `ADP-03`: feedback reaches original logical session without a new process when healthy. `ADP-04`: binary/protocol changes invalidate certification. `ADP-05`: original TUI is the same worker session. `ADP-06`: generic executable registration cannot widen permissions or replace the sealed built-in registry.

## Evidence and references

- **J16 — [Existing Muse adapter](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-core/src/agent/adapters/muse.rs).** Full file. Muse runtime installation/auth/path/version adapter exists. Fallback installer deliberately fails closed.
- **J17 — [Existing sealed agent runtime trait](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-core/src/agent/runtime.rs).** Lines 1–260, including the complete trait. Installation, auth and state behavior, not workflow-turn control. Built-in adapter count in comments is stale; no count inferred from it.
- **E04 — [Claude Code CLI reference](https://code.claude.com/docs/en/cli-reference).** Official interactive, resume and print/stream modes. Modes and flags require exact-version certification. Accessed 2026-10-14.
- **E05 — [Codex App Server reference](https://learn.chatgpt.com/docs/app-server).** Official thread/turn lifecycle, initialization, steer and interrupt contracts. Does not by itself prove an original interactive TUI shares a server session. Accessed 2026-10-14.
- **E06 — [OpenCode server reference](https://opencode.ai/docs/server/).** Official session, message, asynchronous prompt and abort endpoints. Documentation is not proof of deduplication behavior. Accessed 2026-10-14.
- **E07 — [Muse Code official documentation](https://dev.meta.ai/docs/muse-code).** Official interactive/exec/serve/schema modes and installer. Contrast with existing Jackin fail-closed fallback. Accessed 2026-10-14.

## Complete built-in coverage

The registry contains twelve built-ins. Workflow capability rows must cover every `Agent::ALL` entry, not a hard-coded four-agent subset. Dedicated specifications for the remaining agents are available in the README adapter index. All manual runtimes remain compatible. For each workflow capability, a tested supported result or an explicit unsupported/unverified result is mandatory; no blanket “supports all agents” assertion is permitted.

- **J22 — [Built-in agent identity and dispatch registry](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-core/src/agent.rs).** Lines 1–240. Complete Agent enum, Agent::ALL, canonical slugs, auth modes and runtime dispatch. Twelve registered built-in agents; not a workflow capability certificate.
