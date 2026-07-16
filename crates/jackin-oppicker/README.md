# jackin-oppicker

Pure model and planning helpers for the 1Password picker — the side-effect-free half of the operator secret-selection flow. Holds picker state, input handling, and load planning with zero `op` CLI calls.

Split out of the console so the picker's decision logic is unit-testable without touching `op` or the terminal.

## What this crate owns

- Picker state machine (`state`) and input handling (`input`).
- Load/planning helpers (`load`) — what to fetch and how to present it, with no I/O.
- Async receiver adapters (`adapters`) that connect product sources to TermRock's executor-neutral subscription contract.

## Architecture tier and allowed dependencies

**Presentation-adjacent model.** Dependencies include `jackin-core`, `jackin-diagnostics`, `jackin-tui`, Tokio runtime/channels, and TermRock. No `op` or filesystem — those effects live in `jackin-env`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`state.rs`](src/state.rs) | picker state machine | — |
| [`input.rs`](src/input.rs) | input handling | — |
| [`load.rs`](src/load.rs) | load/planning helpers | — |
| [`adapters.rs`](src/adapters.rs) | receiver-to-subscription adapters | inline |

## Public API

Picker state + planning consumed by `jackin-console` (and the side-effect adapters in `jackin-env`). The console/oppicker extraction pattern is the template for future pure-model splits.

## How to verify

```sh
cargo nextest run -p jackin-oppicker
cargo clippy -p jackin-oppicker --all-targets -- -D warnings
```
