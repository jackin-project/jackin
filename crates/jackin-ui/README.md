# jackin-ui

Cross-surface jackin❯ product presentation shared by the console, launch, and capsule surfaces.

## Ownership boundary

- Owns jackin❯-specific compositions shared by at least two surfaces, including:
  - `operator_info` — Debug-info / container-info row policy, `ContainerInfoState`, paint via TermRock `DetailTable`/`Panel`, copy/hyperlink hit geometry, and OSC 8 overlay bytes.
  - `theme` — product Theme/Role accessors and brand/domain Ratatui tokens used by surfaces.
- Does not own neutral widgets, geometry, focus, hover, scroll mechanics, or terminal lifecycle; those belong to TermRock.
- Does not own a surface application model or run loop; those remain under each surface crate's `src/tui/`.
- Does not live in `jackin-core` (L0 domain vocabulary only).

## Structure

| Module | Owns |
|---|---|
| [`operator_info.rs`](src/operator_info.rs) | Cross-surface Debug-info composition and paint |
| [`theme.rs`](src/theme.rs) | Product Theme/Role helpers + brand/domain colors |

## How to verify

```sh
cargo nextest run -p jackin-ui
cargo clippy -p jackin-ui --all-targets -- -D warnings
```
