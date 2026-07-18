# jackin-tui

Cross-surface jackin❯ product presentation shared by the console, launch, and capsule surfaces.

## Ownership boundary

- Owns jackin❯-specific compositions shared by at least two surfaces, including:
  - `operator_info` — Debug-info / container-info row policy, `ContainerInfoState`, paint via TermRock `DetailTable`/`Panel`, copy/hyperlink hit geometry, and OSC 8 overlay bytes.
  - `tokens` — Ratatui adapters for product-owned brand/domain color tokens (brand pill, menu chips, status accents).
  - `ModalOutcome` — product workflow lifecycle shared by jackin❯ modal compositions.
- Does not own neutral widgets, geometry, focus, hover, scroll mechanics, Theme/Role tables, or terminal lifecycle; those belong to TermRock. Surfaces resolve neutral roles via `termrock::Theme::default()` / `Role` directly.
- Does not own a surface application model or run loop; those remain under each surface crate's `src/tui/`.
- Does not live in `jackin-core` (L0 domain vocabulary only).

## Architecture tier and allowed dependencies

**L1 presentation.** Workspace deps: `jackin-brand`, `jackin-core`. External: `termrock`, `ratatui`, `crossterm`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root | — |
| [`modal_outcome.rs`](src/modal_outcome.rs) | Shared jackin❯ modal-workflow outcome vocabulary | — |
| [`operator_info.rs`](src/operator_info.rs) · [`operator_info/`](src/operator_info) | Cross-surface Debug-info composition and paint | [`tests.rs`](src/operator_info/tests.rs) |
| [`tokens.rs`](src/tokens.rs) · [`tokens/`](src/tokens) | Product-owned brand/domain Ratatui colors | [`tests.rs`](src/tokens/tests.rs) |

## How to verify

```sh
cargo nextest run -p jackin-tui
cargo clippy -p jackin-tui --all-targets -- -D warnings
```
