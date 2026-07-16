# jackin-ui

Cross-surface jackin❯ product presentation shared by the console, launch, and capsule surfaces.

## Ownership boundary

- Owns jackin❯-specific compositions shared by at least two surfaces.
- Does not own neutral widgets, geometry, focus, hover, scroll, themes, or terminal lifecycle; those belong to TermRock.
- Does not own a surface application model or run loop; those remain under each surface crate's `src/tui/`.

## How to verify

```sh
cargo nextest run -p jackin-ui
cargo clippy -p jackin-ui --all-targets -- -D warnings
```
