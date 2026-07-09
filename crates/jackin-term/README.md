# jackin-term

Owned terminal model for the `jackin-capsule` re-emitting PTY multiplexer — the grid, parser-perform sink, damage tracking, passthrough events, and snapshot/observation APIs behind the [Capsule Terminal Model](../../docs/content/docs/reference/capsule/terminal-model.mdx).

The full design record — why `vt100` was retired, the candidate survey, the borrow/re-implement ledger, the current Ratatui/emit contract, scrollback-retention semantics, and the correctness guarantees — lives in that doc. This README is the current-state map of the crate.

## What this crate owns

- The VT/ANSI parser-perform sink over `vte`: bytes → grid mutation + typed passthrough events.
- The `DamageGrid` cell model: cursor, modes, styles, alternate screen, scrollback, wide-cell/grapheme-cluster handling, and dirty-row damage recorded at mutation time.
- Snapshot/observation APIs (`GridView`, `GridSnapshot`) the capsule renders from, plus width and passthrough helpers.

## Architecture tier and allowed dependencies

L2 infrastructure crate. Allowed workspace dependencies: `jackin-core`, `jackin-diagnostics`. No presentation, no `ratatui`, no host effects — only the model + diff/emit surface the capsule consumes.

## Structure

- `src/grid.rs` / `src/grid/` — `DamageGrid` cell model, scrollback, damage
- `src/damage.rs` / `src/damage/` — dirty-row tracking
- `src/cell.rs` — packed cell (`CompactString` grapheme storage)
- `src/passthrough.rs` — typed `PassthroughEvent` stream
- `src/snapshot.rs` / `src/snapshot/`, `src/width.rs` / `src/width/` — snapshot/observation + width helpers
- `tests/conformance.rs`, `tests/fixtures/` — conformance replay harness + corpus
- `fuzz/`, `benches/`, `examples/` — fuzz target, criterion benches, examples

## Public API

`DamageGrid`, the parser-perform entry point, `GridView` / `GridSnapshot` observation APIs, `PassthroughEvent`, and the width/snapshot helpers — consumed by `jackin-capsule`. The crate is pure-Rust: no `unsafe`, no FFI, no host-side effects (all mutation is in-memory).

## How to verify

```sh
cargo nextest run -p jackin-term
cargo clippy -p jackin-term --all-targets -- -D warnings
```

See [../AGENTS.md](../AGENTS.md) for workspace-wide Rust rules and [../../AGENTS.md](../../AGENTS.md) for repo rules. Design rationale and prior art: [Capsule Terminal Model](../../docs/content/docs/reference/capsule/terminal-model.mdx).
