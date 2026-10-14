# B01 — Agent block

**Status:** Proposed block contract. Every field in the [common node contract](../architecture/02-domain-contracts.md) is required after template/default resolution: identity, typed inputs/outputs, dependencies, permissions, limits, retry and completion.

## Purpose

Execute one bounded unit of real coding-agent work in a Capsule-managed CLI session, retaining the session for later feedback and operator inspection. A completed Agent block is not accepted implementation.

## API: configuration and ports

`kind: agent`. Required configuration: `agentRef`, `roleRef`, `repositoryRef`, `model/provider` selection, `contextRefs`, `instructionRef`, `mode`, `requiredCapabilities`, `turnLimit`, `completionPolicy`, `sessionRetention`. Optional: an explicitly certified slash/goal command and an authorized fallback chain.

Inputs: `context: context-bundle/v1`, `repository: repository-ref/v1`, `instructions: text/v1` or a pinned document artifact. Outputs: `session: session-ref/v1`, `repository: repository-ref/v1`, `turn: agent-turn-result/v1`, optional declared `artifact-set/v1`. Required output contracts include size/schema limits. A session output is an opaque host reference, never a lease token or broad control socket.

## Functionality

Resolve the certified adapter and effective launch grant, provision or reconcile the exact owned instance, create the original Capsule session, acquire scheduler input ownership and submit the immutable instruction payload. Capture transport/vendor events and declared outputs. A first prompt supplied at startup still receives a durable command ID and recorded launch/input correlation; avoid double delivery by also pasting it later.

The turn ends only according to the declared observable completion boundary: a certified vendor event/process result or an explicit bounded operator/manual path for a limited adapter. Screen readiness may synchronize typing but cannot prove successful software implementation. Unsupported autonomous completion capability rejects an autonomous block rather than leaving an unbounded wait.

Keep the healthy session alive when repair or live inspection depends on it. References are read-only. Distinct writable Agent blocks get distinct instances unless sharing is explicitly declared and authorized; sharing a role does not imply sharing an instance.

## Permissions, errors and limits

Only the effective repository/network/credential/tool grant applies. Host deadline and resource limits cover any vendor subagents or goal loops. Auth failures, missing models, lease conflicts, unknown delivery and adapter capability failures remain distinct. Agent prose cannot create new permissions or skip downstream verification.

## Build instructions and acceptance

Wire this executor to C04/C05/C06 rather than spawning a new PTY. Persist launch and prompt intents before dispatch. Test autonomous startup, exact long instructions/attachments, original TUI attachment, retained session output, no duplicated initial prompt and the distinction between turn completion and final acceptance. Test IDs: `ADP-02`, `ADP-05`, `CAP-03`, `GATE-01`.

## Implementation ownership

See [C05 — Agent execution adapters and capability registry](../components/05-agent-adapters.md). Shared behavior is implemented once in that component and exposed through this block; do not create a separate executor stack.
