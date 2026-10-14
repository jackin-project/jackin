# A02 — Codex workflow adapter

**Status:** Proposed Jackin workflow adapter contract. Upstream documentation is not a passed Jackin runtime certification. Shared purpose/API semantics are defined by [C05](../components/05-agent-adapters.md), [C06](../components/06-capsule-session-control.md) and [C07](../components/07-input-leases-attachment.md).

## Purpose and observed upstream surface

Use real Codex CLI execution while retaining original-session inspection. The official App Server protocol documents initialization, thread/turn operations, steering guarded by the expected current turn and interruption. That protocol alone does not prove an arbitrary server process and an independently launched TUI share one live session. [E05](#evidence-and-references).

## API and capability mapping

C05 operations wrap either a certified interactive PTY profile or a certified shared-session structured profile. A profile names the binary/protocol digest and proves the session topology. Do not start one Codex worker for automation and a second for display.

| Capability | Target mapping / rule |
|---|---|
| Structured startup | Complete protocol initialization before thread or turn requests when using App Server. |
| Session identity | Persist vendor thread ID separately from logical session and Capsule epoch/address. |
| Turn input | Correlate a host command with the exact thread and returned turn/message identity. |
| Steering | Require the expected active turn and scheduler lease; reject a mismatch rather than starting a replacement turn. |
| Interrupt | Target the exact active turn and wait for observable state; cancellation is not success. |
| Goal input | Explicit capability/rollout check; host bounds still govern. |
| Original TUI | Mandatory proof of actual shared session for structured profiles; otherwise use interactive PTY. |

## Required functionality

Thread creation/resume and turn submission are distinct effect intents with their own reconciliation keys. Initialize the transport once per connection as required, but do not create another thread merely because the connection dropped. Persist normalized completed/failed/interrupted states without treating any of them as accepted implementation.

Use structured review support only as a transport feature for a separately isolated reviewer instance/session. A review within the implementer’s own mutable environment cannot satisfy C13 independence. Model, effort, cwd and permissions remain plan-resolved; a steer request cannot silently alter them.

For the interactive profile, Capsule owns process/PTY and all input. Turn observations may use certified hooks/status evidence, but terminal screen text is only synchronization/debug evidence. If a profile cannot reconcile ambiguous delivery, it must expose `delivery_unknown` and require resolution rather than claim exactly-once processing.

## Build instructions

Inspect existing Codex role/runtime/protocol integration before adding code. Build profile-specific capability manifests and shared-session topology tests. Implement protocol correlation with exact version fixtures, then lease mediation over every structured and terminal ingress. Keep the interactive PTY path available when shared-server/TUI equivalence is unproven. No new general-purpose LLM SDK is required.

## Acceptance tests

`CODEX-01`: proper initialization/session targeting. `CODEX-02`: stale expected turn rejects steering. `CODEX-03`: operator sees and controls the same actual session automation drives. `CODEX-04`: reconnect does not silently fork/create a new thread. `CODEX-05`: failed/interrupted/completed vendor events remain distinct from acceptance. `CODEX-06`: a review cannot reuse implementer mutable state. `CODEX-07`: unsupported goal/profile combinations fail admission.

## Evidence and references

- **E05 — [Codex App Server reference](https://learn.chatgpt.com/docs/app-server).** Official thread/turn lifecycle, initialization, steer and interrupt contracts. Does not by itself prove an original interactive TUI shares a server session. Accessed 2026-10-14.
- **E15 — [Codex goals](https://developers.openai.com/codex/use-cases/follow-goals/).** Official goal surface; capability and rollout must be checked for the selected build. Host loop/budget limits still apply. Accessed 2026-10-14.
- **J17 — [Existing sealed agent runtime trait](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-core/src/agent/runtime.rs).** Lines 1–260, including the complete trait. Installation, auth and state behavior, not workflow-turn control. Built-in adapter count in comments is stale; no count inferred from it.
