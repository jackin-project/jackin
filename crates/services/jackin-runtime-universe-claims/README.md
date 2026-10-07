# jackin-runtime-universe-claims

Construct-boundary
claim types and
their shared file
machinery.

## What this crate owns

- Claims
  (`claims`):
  `EntryClaim`,
  `ExitClaim`,
  `StartKind` —
  entry leases and
  the single-consumer
  exit claim.
- Boundary
  (`boundary`):
  lock, generation,
  state and pending
  files under the
  universe authority.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`claims.rs`](src/claims.rs) | claim types | `universe` suites |
| [`boundary.rs`](src/boundary.rs) | file machinery | `universe` suites |

## Public API

`claims::EntryClaim`,
`claims::ExitClaim`,
`claims::StartKind`,
`boundary::boundary_lock`,
`boundary::boundary_work`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-universe
cargo clippy -p jackin-runtime-universe-claims --all-targets -- -D warnings
```
