# jackin-runtime-universe

Construct-entry/exit
boundary tracking for
the operator span.

## What this crate owns

- Entry (`universe`):
  `claim_entry`,
  `EntryClaim`,
  `StartKind` — pending
  leases until the role
  container exists.
- Exit (`universe`):
  `observe_exit`,
  `release_entry_if_idle`,
  `take_exit_claim` —
  single-consumer close.
- Rituals (`universe`):
  `force_boundary_intro_enabled`,
  `mark_start`,
  `env_flag_enabled`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`universe.rs`](src/universe.rs) | boundaries | `universe/tests/` |
| [`universe/`](src/universe/) | test suites | `universe/tests/` |

## Public API

`universe::claim_entry`,
`universe::EntryClaim`,
`universe::StartKind`,
`universe::release_entry_if_idle`,
`universe::observe_exit`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-universe
cargo clippy -p jackin-runtime-universe --all-targets -- -D warnings
```
