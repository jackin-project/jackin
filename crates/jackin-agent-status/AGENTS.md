# AGENTS.md — jackin-agent-status

Agent runtime status authority — evidence-driven state machine for what an agent is doing.

## Hard rules (this crate)

- **Tier & dependencies:** domain/status crate. Allowed workspace deps: `jackin-core`, `jackin-protocol`. No infrastructure or presentation dependencies — keep status logic pure arbitration over evidence types.
- **Keep `README.md` current:** update it when structure, public API, the evidence model, or the arbitration rules change (see `crates/AGENTS.md`).
- **Evidence-driven, not timer-driven.** Status decisions run over collected evidence; do not reintroduce timer-only heuristics. Anti-flicker and debounce belong in gating, not scattered across detectors.
- **Detectors stay under test.** Screen/process-detector regression tests and anti-flicker behavior are the anchors; a status-logic change keeps them green and adds coverage for the new transition.

## What lives here vs elsewhere

- This crate owns: evidence types, process/screen signals, decision rules/policy, gating, arbitration.
- The daemon that *drives* status + the PTY it reads live in `jackin-capsule`. Operator surfacing of status lives in `jackin-console`/desktop hub work.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
