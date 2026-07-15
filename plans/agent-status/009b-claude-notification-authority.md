# Plan 009b: Productionize Claude Notification authority

## Status

- **Implementation status**: **RESIDUAL** — production payload capture/enrichment, partial-authority mapping, pending-permission suppression, and screen-blocked override tests shipped; live in-container permission/idle/elicitation wait-edge and ordering validation remains open
- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: 009 live ordering validation
- **Category**: direction / semantic authority

## Why this matters

Plan 009 maps documented Claude `Notification` wait states to partial authority while preserving heartbeat-only
behavior for order-fragile lifecycle hooks. Productionizing this requires live validation that
`permission_prompt`, `idle_prompt`, and `elicitation_*` notifications arrive at the same state edges that the
docs describe, without resurrecting the Decision 0a Stop/Subagent ordering hazard.

## Scope

In scope: real in-container Claude Code runs for permission, idle, and elicitation cases; reporter payload
capture; regression tests for pending-permission suppression; and screen-blocked override coverage.

Out of scope: promoting `Stop`, `SubagentStop`, `PostToolUse`, or other lifecycle hooks to state authority.

## STOP conditions

- `idle_prompt` can arrive while a permission or elicitation prompt is still visible and unresolved.
- Notification payloads do not expose enough type information to distinguish permission, idle, and elicitation
  wait states.
- Claude Code changes hook semantics so Notification is no longer a wait-state signal.
