# jackin-console-oppicker

Pure model and planning helpers for the 1Password picker — the side-effect-free half of the operator secret-selection flow. Holds picker state, input handling, and load planning with zero `op` CLI calls.

Split out of the console so the picker's decision logic is unit-testable without touching `op` or the terminal.

## What this crate owns

- Picker state machine (`state`) and input handling (`input`).
- Load/planning helpers (`load`) — what to fetch and how to present it, with no I/O.

## Architecture tier and allowed dependencies

**Presentation-adjacent pure model.** Allowed workspace dependencies: `jackin-core`, `jackin-diagnostics`, `jackin-tui`. No `op`, no `tokio`, no filesystem — the `op` side-effects live in `jackin-env`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`state.rs`](src/state.rs) | picker state machine | — |
| [`input.rs`](src/input.rs) | input handling | — |
| [`load.rs`](src/load.rs) | load/planning helpers | — |

## Public API

Picker state + planning consumed by `jackin-console` (and the side-effect adapters in `jackin-env`). The console/oppicker extraction pattern is the template for future pure-model splits.

## How to verify

```sh
cargo nextest run -p jackin-console-oppicker
cargo clippy -p jackin-console-oppicker --all-targets -- -D warnings
```

