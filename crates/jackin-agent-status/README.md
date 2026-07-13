# jackin-agent-status

Agent runtime status authority. Owns all state-machine logic for determining what an agent is doing at any moment — replacing the old timer-based heuristics with an evidence-driven arbitration model consumed by the capsule daemon and operator surfaces.

## What this crate owns

- Evidence collection (`evidence`, `process`, `screen`) — the signals that feed a status decision.
- The status-decision state machine: rules (`rules`), policy (`policy`), gating (`gating`), and arbitration (`arbitrate`).

## Architecture tier and allowed dependencies

**Domain/status crate** above the leaf. Allowed workspace dependencies: `jackin-core`, `jackin-protocol`. No infrastructure or presentation dependencies — status logic is pure arbitration over evidence types.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`evidence.rs`](src/evidence.rs) · [`evidence/`](src/evidence) | status evidence types + collection | [`tests.rs`](src/evidence/tests.rs) |
| [`process.rs`](src/process.rs) · [`process/`](src/process) | process sampling signals | [`tests.rs`](src/process/tests.rs) |
| [`screen/`](src/screen) | screen-state signals | — |
| [`rules.rs`](src/rules.rs) · [`rules/`](src/rules) | decision rules | [`tests.rs`](src/rules/tests.rs) |
| [`policy.rs`](src/policy.rs) · [`policy/`](src/policy) | status policy | [`tests.rs`](src/policy/tests.rs) |
| [`gating.rs`](src/gating.rs) · [`gating/`](src/gating) | anti-flicker gating + debounce | [`tests.rs`](src/gating/tests.rs) |
| [`arbitrate.rs`](src/arbitrate.rs) · [`arbitrate/`](src/arbitrate) | final status arbitration | [`tests.rs`](src/arbitrate/tests.rs) |
| [`tests.rs`](src/tests.rs) | integration tests | — |

## Public API

Status-decision types and the arbitration entry point consumed by `jackin-capsule` and surfaced to operators. Screen-detector tests and anti-flicker behavior are the regression anchors — keep them green.

## How to verify

```sh
cargo nextest run -p jackin-agent-status
cargo clippy -p jackin-agent-status --all-targets -- -D warnings
```
