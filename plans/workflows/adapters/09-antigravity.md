# A09 — Antigravity workflow adapter

**Status:** Proposed Jackin workflow adapter contract. Upstream documentation is not a passed Jackin runtime certification. Shared purpose/API semantics are defined by [C05](../components/05-agent-adapters.md), [C06](../components/06-capsule-session-control.md) and [C07](../components/07-input-leases-attachment.md).

## Purpose and verified foundation

Expose Jackin's registered `antigravity` runtime through the common workflow adapter while preserving its existing installation/auth/role behavior. The inspected `Agent::ALL` and runtime dispatch include this agent. API-key selection is `GEMINI_API_KEY`; the registry permits Sync, ApiKey and Ignore modes. This establishes registration and configuration shape, not successful autonomous workflow execution. [J22](#evidence-and-references).

## API

Implement C05's `probe`, `prepare_session`, `prepare_input`, `interpret_event`, `interrupt` and `recover` against a profile pinned by binary digest/version, role/backend, mode and provider/model tuple. Reuse the registered built-in adapter rather than replacing it with a generic executable definition. Return supported/unsupported/unverified separately for initial input, attachments, safe turn boundaries, follow-up, steering, structured acknowledgement, interruption, resume and original TUI.

## Required functionality

The registry identifies a separate Antigravity runtime. Do not substitute Gemini CLI simply because the API-key environment name overlaps. Preserve its actual runtime executable and capability identity.

Keep the actual interactive process under Capsule. Submit exact immutable task/reference content with C06 command IDs and C07 input leases. Healthy feedback must target the original logical/vendor session when supported. Model narration, process readiness and terminal silence never constitute independent verification. Agent-specific settings cannot widen sandbox or publication permissions.

No upstream workflow protocol or resume/goal command for this agent was independently verified during this audit. Therefore no such command is invented here. The adapter must use build-specific primary documentation/help/schema and real-binary fixtures before advertising those capabilities. Until then, plans requiring unverified behavior are rejected with the capability named; existing manual use remains available unchanged.

## Build instructions

Inspect the existing `antigravity` adapter and all launch/auth call sites at the implementation base. Record the exact supported executable and upstream documentation version. Add an interactive capability profile, fixture-driven input/observation, interruption and same-session tests. Add a structured transport only if it preserves the required original TUI and can be fenced. Do not copy installation, credential or PTY code into the workflow service.

## Acceptance tests

`ANTIGRAVITY-01`: existing built-in identity/auth/role behavior is preserved. `ANTIGRAVITY-02`: exact prompt/references and original live TUI are demonstrated. `ANTIGRAVITY-03`: same-session follow-up is certified or explicitly unavailable. `ANTIGRAVITY-04`: takeover fences every input route. `ANTIGRAVITY-05`: crash, resume, goal and steering claims match the tested capability manifest. `ANTIGRAVITY-06`: no agent/provider setting expands the host grant.

## Evidence and references

- **J22 — [Built-in agent identity and dispatch registry](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-core/src/agent.rs).** Lines 1–240. Complete Agent enum, Agent::ALL, canonical slugs, auth modes and runtime dispatch. Twelve registered built-in agents; not a workflow capability certificate.
- **J17 — [Existing sealed agent runtime trait](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-core/src/agent/runtime.rs).** Lines 1–260, including the complete trait. Installation, auth and state behavior, not workflow-turn control. Built-in adapter count in comments is stale; no count inferred from it.
