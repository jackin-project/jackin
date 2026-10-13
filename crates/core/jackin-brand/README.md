# jackin-brand

Renderer-neutral jackin❯ identity and product-domain color tokens shared by CLI output, launch animation, and TUI adapters.

## What this crate owns

- The compact `Rgb` value and `owo-colors` adapter used by non-Ratatui output.
- Brand, launch-animation, and product-domain color tokens.

Neutral component colors remain in TermRock. Ratatui `Color` adapters remain in `jackin-tui`.

## Architecture tier and allowed dependencies

**T0 foundation.** No workspace dependencies. External dependency: `owo-colors`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | RGB value, renderer adapter, and product tokens | — |

## Public API

Consumers import named tokens and `owo_rgb` from the crate root. Presentation crates adapt these renderer-neutral values rather than duplicating RGB literals.

## How to verify

```sh
cargo nextest run -p jackin-brand
cargo clippy -p jackin-brand --all-targets -- -D warnings
```
