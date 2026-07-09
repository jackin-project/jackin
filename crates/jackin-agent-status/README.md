# jackin-agent-status

Agent runtime status authority. Owns all state-machine logic for determining what an agent is doing at any moment — replacing the old timer-based heuristics with an evidence-driven arbitration model consumed by the capsule daemon and operator surfaces.

## What this crate owns

- Evidence collection (`evidence`, `process`, `screen`) — the signals that feed a status decision.
- The status-decision state machine: rules (`rules`), policy (`policy`), gating (`gating`), and arbitration (`arbitrate`).

## Architecture tier and allowed dependencies

**Domain/status crate** above the leaf. Allowed workspace dependencies: `jackin-core`, `jackin-protocol`. No infrastructure or presentation dependencies — status logic is pure arbitration over evidence types.

## Structure

- `src/evidence.rs` / `src/evidence/` — status evidence types + collection
- `src/process.rs` / `src/process/`, `src/screen.rs` (under `screen/`) — process + screen signals
- `src/rules.rs` / `src/rules/`, `src/policy.rs` / `src/policy/` — decision rules + policy
- `src/gating.rs` / `src/gating/`, `src/arbitrate.rs` / `src/arbitrate/` — gating + arbitration
- `src/lib.rs`, `src/tests.rs` — root + tests

## Public API

Status-decision types and the arbitration entry point consumed by `jackin-capsule` and surfaced to operators. Screen-detector tests and anti-flicker behavior are the regression anchors — keep them green.

## How to verify

```sh
cargo nextest run -p jackin-agent-status
cargo clippy -p jackin-agent-status --all-targets -- -D warnings
```

See [../AGENTS.md](../AGENTS.md) for workspace-wide Rust rules and [../../AGENTS.md](../../AGENTS.md) for repo rules.
