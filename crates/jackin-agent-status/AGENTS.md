# AGENTS.md — jackin-agent-status

Agent runtime status authority — evidence-driven state machine for what an agent is doing.

## Rules (this crate)

- Evidence-driven, not timer-driven: status decisions run over collected evidence; do not reintroduce timer-only heuristics. Anti-flicker and debounce live in gating, not scattered across detectors.
- Detectors stay under test: screen/process-detector regression tests and anti-flicker behavior are the anchors — a status-logic change keeps them green and adds coverage for the new transition.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
