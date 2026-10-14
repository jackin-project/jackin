# A03 — OpenCode workflow adapter

**Status:** Proposed Jackin workflow adapter contract. Upstream documentation is not a passed Jackin runtime certification. Shared purpose/API semantics are defined by [C05](../components/05-agent-adapters.md), [C06](../components/06-capsule-session-control.md) and [C07](../components/07-input-leases-attachment.md).

## Purpose and observed upstream surface

Use the real OpenCode runtime with a native TUI under Capsule. Official documentation exposes server session/message/abort operations and a native terminal client that can attach to running server state. This is a useful candidate for structured control plus live TUI, but the selected version/topology still needs certification. [E06](#evidence-and-references), [E14](#evidence-and-references).

## API and capability mapping

C05’s session and input operations map to the exact pinned server contract. Persist server incarnation, vendor session ID, Capsule physical session address, host command ID and any vendor message ID. An asynchronous HTTP acceptance is a transport acknowledgement, not completed work.

| Capability | Target behavior |
|---|---|
| Startup | Start approved server/runtime through Jackin, then the original native TUI under Capsule for the selected session. |
| Structured prompt | Submit to an exact session, record message identity and correlate stream events. |
| Native attachment | Keep the actual native client/session alive from the interactive block’s start. |
| Follow-up | Address the same session; serialize with terminal input under one lease authority. |
| Abort/resume | Exact session/turn behavior certified against the pinned server contract. |
| Deduplication | Test message-ID semantics; an optional ID field is not proof that retries are safe. |

## Required functionality

Bind server interfaces to the approved isolated network/socket boundary, authenticate clients and keep credentials out of URLs and logs. The host accesses the structured transport through a scoped channel, not an unauthenticated externally reachable port. Agent code in the worker must not acquire host operator API authority from server credentials.

All terminal and HTTP input must obey the same current lease. A native TUI that writes directly to a server in a way Capsule cannot fence requires an adapter/proxy or server-side enforcement; otherwise that topology is not eligible for exclusive-control certification. Do not mark the UI locked while automation can still submit through another endpoint.

Persist result/event cursors where the protocol supports replay. Treat lost event history explicitly and inspect session state before resuming. The same filesystem belongs to one instance even if the server has several sessions; independent implementers/reviewers require independent instances unless shared-state policy explicitly allows otherwise.

## Build instructions

Pin one documentation/protocol generation; do not mix legacy and newer version endpoints in one adapter. Add authenticated transport, exact session selection and native TUI startup. Add a command/message reconciliation fixture, then all-ingress lease tests. Validate that the operator’s original native terminal reflects the same prompt, files and turn as structured automation.

## Acceptance tests

`OPENCODE-01`: original native TUI and API target the same session. `OPENCODE-02`: wrong/unauthenticated session access fails. `OPENCODE-03`: HTTP and terminal input cannot bypass exclusive lease. `OPENCODE-04`: lost asynchronous reply is reconciled without assuming idempotency. `OPENCODE-05`: server restart/session recovery changes identity correctly. `OPENCODE-06`: independent reviewers use independent state.

## Evidence and references

- **E06 — [OpenCode server reference](https://opencode.ai/docs/server/).** Official session, message, asynchronous prompt and abort endpoints. Documentation is not proof of deduplication behavior. Accessed 2026-10-14.
- **E14 — [OpenCode web and terminal attachment](https://opencode.ai/docs/web/).** Official terminal attachment to a running server and shared session state. Exact native-TUI workflow must still be tested. Accessed 2026-10-14.
- **J07 — [Shared Capsule contracts](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-protocol/src/lib.rs).** Lines 1–280. Session capability and identity environment, scoped credential contracts, CapsuleConfig and exit preservation choices.
- **J17 — [Existing sealed agent runtime trait](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-core/src/agent/runtime.rs).** Lines 1–260, including the complete trait. Installation, auth and state behavior, not workflow-turn control. Built-in adapter count in comments is stale; no count inferred from it.
