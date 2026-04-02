# Matrix-Styled CLI & Cinematic Rain

**Date:** 2026-04-02
**Status:** Approved
**Scope:** CLI help styling, help banner, Matrix rain visual improvements

## Overview

Two visual improvements to jackin:

1. **Styled CLI help** — phosphor-themed clap colors, rain/jack banner in `--help`, Matrix-flavored command descriptions
2. **Cinematic Matrix rain** — per-cell age tracking for smooth color gradients, natural trail fade-out, age-based character mutation

## Part 1: CLI Banner & Help Styling

### Banner

Displayed via clap's `before_help` attribute. Static text (no ANSI color — clap handles styling separately):

```
    │ │╷│ │╷│ ╷  │╷│ │╷│ │╷│
    │ ╵│ │╵│ ╵ ╷ ╵│ │╵│ │╵│
    ╵  ╵ ╵ ╵  │  ╵ ╵ ╵ ╵ ╵
               ╵
          j a c k i n
       operator terminal
```

### Clap Styles

phosphor green palette via `Styles::styled()`:

- **Headers** (`Usage:`, `Commands:`, `Options:`) — `BrightGreen`, bold
- **Literals** (command names, flag names) — `Green`, bold
- **Placeholders** (`<selector>`, `<container>`) — `Green`
- **Valid values** — `BrightGreen`
- **Invalid/errors** — `Red`, bold

### Command Descriptions

Rewrite doc comments with Matrix-flavored language:

| Element | Description |
|---------|-------------|
| Top-level about | `"Send agents into the Matrix"` |
| `Load` | `"Jack an agent into the Matrix"` |
| `Hardline` | `"Reattach to a running agent"` |
| `Eject` | `"Pull an agent out of the Matrix"` |
| `Exile` | `"Pull every agent out"` |
| `Purge` | `"Delete persisted state for an agent class"` |
| `Config` | `"Operator configuration"` |
| `--no-intro` | `"Bypass the construct sequence"` |
| `--debug` | `"Show raw signal output"` |
| `--all` on Eject | `"Pull every instance of this class"` |
| `--purge` on Eject | `"Delete persisted state after ejection"` |
| `--all` on Purge | `"Delete state for every instance of this class"` |
| `Mount Add` | Mount subcommand descriptions stay functional (not thematic) |

## Part 2: Cinematic Matrix Rain

### Problem

The current `digital_rain()` implementation has visible limitations:

1. Fixed trail length of 8 cells — trails vanish abruptly
2. Only 4 color steps — visible banding between gradient stages
3. Color based on distance from head, not cell age — no independent cell lifecycle
4. Low mutation rate (1-in-5) — trails feel static
5. Uniform respawn timing — columns restart in waves

### Solution: Per-Cell Age Tracking

Replace `grid: Vec<Vec<char>>` with a cell struct:

```rust
struct RainCell {
    ch: char,
    age: u16,
}
```

Grid becomes `Vec<Vec<Option<RainCell>>>`.

Each frame:

1. Age every existing cell by +1
2. Advance each column's head, write a fresh cell (age 0) at the new position
3. Cells beyond max age (25 frames) become `None` — natural fade-out
4. Cells mutate with age-based probability

### Color Gradient — 7 Steps

| Age | Color RGB | Role |
|-----|-----------|------|
| 0 | `(255, 255, 255)` | Head — brightest point |
| 1-2 | `(180, 255, 180)` | Fresh trail |
| 3-5 | `(0, 255, 65)` | Core trail (matrix green) |
| 6-10 | `(0, 200, 50)` | Mid fade |
| 11-16 | `(0, 140, 30)` | Dim green |
| 17-24 | `(0, 80, 18)` | Dark green, nearly gone |
| 25+ | `None` | Cell cleared |

### Mutation — Age-Based Probability

- Age 0-2: 30% chance per frame (head area flickers intensely)
- Age 3-10: 15% per frame (active trail shimmers)
- Age 11+: 5% per frame (fading trail mostly stable)

### Column Behavior

- **Speed:** unchanged (1-3 rows per frame, per column)
- **Respawn wait:** 3-15 frames after head exits bottom (was 0-7). Creates more organic density variation
- **Initial stagger:** randomize start positions more aggressively so frame 0 doesn't look uniform

### Parameters Unchanged

- Grid size: 70 columns × 18 rows
- Frame timing: 60ms
- Intro duration: 2000ms
- Outro duration: 1500ms
- Character set: ASCII (`0-9A-Za-z@#$%&*<>{}[]|/\~`)
- PRNG: xorshift with seed `0xDEAD_BEEF_CAFE_1337`

## Files Changed

### `src/cli.rs`

- Add `HELP_STYLES` constant with phosphor green color palette
- Add `#[command(styles = HELP_STYLES)]` to `Cli` struct
- Add `#[command(before_help = BANNER)]` with rain/jack banner
- Rewrite all doc comments to Matrix-flavored descriptions
- Existing tests continue to pass (they parse commands, not help text)

### `src/tui.rs`

- Add `RainCell` struct with `ch: char` and `age: u16` fields
- Rewrite `digital_rain()` function with per-cell age tracking
- 7-step color gradient based on cell age
- Age-based mutation probability
- Organic respawn timing (3-15 frame wait)
- No changes to any other function: `matrix_intro()`, `matrix_outro()`, `simple_outro()`, `print_config_table()`, `step_shimmer()`, `step_fail()`, `print_deploying()`, `print_logo()`, `fatal()`, `set_terminal_title()`, `clear_screen()`

### No Other Files

No new dependencies. `owo-colors` and `clap` with `color` feature already in `Cargo.toml`.
