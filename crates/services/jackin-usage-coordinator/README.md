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

## Durable versions

The broker protocol is wire `v8`; its account-identity values distinguish
provider-issued IDs and handles from local source handles and unverified
legacy handles. The projection payload remains `UsageProjectionSchemaV1`.
These are separate from the durable projection envelope (schema `3`) and
per-account envelope (schema `2`). Broker monitor state is schema `4`, and
statusline input is schema `2`; neither is migrated by the projection store.

Projection envelope schema `2` is read only at the broker's explicit startup
migration boundary. That migration canonicalizes provider IDs, marks legacy
account identity provenance as `UnverifiedHandle`, and atomically writes
schema `3` before exposing the projection. Ordinary reads require schema `3`.
Schema `1`, malformed state, and unsupported older state are quarantined;
valid future schema versions fail closed while preserving their bytes.

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
mbx +1.97.1 nextest run -p jackin-usage-coordinator --locked --offline
mbx +1.97.1 clippy -p jackin-usage-coordinator --all-targets --locked --offline -- -D warnings
```
