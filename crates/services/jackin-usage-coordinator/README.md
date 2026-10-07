# jackin-usage-coordinator

Per-account single-flight usage refresh generations: one worker per
account capability joins concurrent refresh requests into a single
probe generation, with file-backed generation and projection state.

## What this crate owns

- Generations (`jobs`, `worker`, `finish`, `join`, `refresh`,
  `reconcile`, `upkeep`, `cadence`): single-flight scheduling and
  terminal history.
- Probe seam (`outcome::UsageProviderExecutor`): probe execution is
  implemented once by the T4 host broker; the coordinator never
  names a vendor.
- State (`state`): file-backed account and projection envelopes
  behind `AccountStateStore`.
- Policy (`policy`): refresh cadence policy.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | [`tests.rs`](src/tests.rs) |
| `jobs`/`worker`/`finish` | generation lifecycle | `tests/` cases |
| [`state/`](src/state.rs) | file-backed envelopes | `state/tests/` |

## Public API

`UsageCoordinator`, `UsageCoordinatorConfig`, `UsageCapabilitySet`,
`ProviderProbeOutcome`, `UsageProviderExecutor`, `policy`,
`AccountStateStore`, `FileAccountStateStore`,
`FileProjectionStateStore`, `ProjectionStateEnvelope`,
`AccountStateEnvelope`, `ProjectionAlias`, `StateStoreError`.

## How to verify

```sh
cargo nextest run -p jackin-usage-coordinator
cargo clippy -p jackin-usage-coordinator --all-targets -- -D warnings
```
