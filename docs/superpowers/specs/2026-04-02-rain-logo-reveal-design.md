# Rain Logo Reveal

**Date:** 2026-04-02
**Status:** Approved
**Scope:** Digital rain animation gains a logo reveal phase in `src/tui.rs`

## Overview

The `digital_rain()` function gains a second phase. After pure rain, the animation transitions into a reveal phase where rain dies out and the jackin banner materializes in the center of the grid. The logo holds briefly, then the screen clears and the existing typewriter sequence continues.

## Banner

The banner that materializes from the rain:

```
│ │╷│ │╷│ ╷  │╷│ │╷│ │╷│
│ ╵│ │╵│ ╵ ╷ ╵│ │╵│ │╵│
╵  ╵ ╵ ╵  │  ╵ ╵ ╵ ╵ ╵
           ╵
      j a c k i n
   operator terminal
```

Positioned vertically centered in the 70×18 grid, horizontally centered.

## Phases

### Phase 1 — Pure Rain (0–2000ms)

Unchanged from the current per-cell age tracking implementation. No new behavior.

### Phase 2 — Reveal (2000–3000ms)

- The banner characters are pre-mapped to grid positions (centered in the 70×18 grid)
- Rain columns stop spawning new heads — no new drops start; existing heads continue falling off the bottom
- Non-banner cells continue aging normally and fade to `None` (they die naturally since no new heads spawn)
- Banner cells gradually "lock in":
  - Each banner cell picks a random frame within the reveal window to flip (assigned at the start of phase 2 using the existing xorshift PRNG)
  - When a cell's flip frame arrives, the cell changes to the correct banner character and stops aging/mutating
  - Locked cells render in matrix green `(0, 255, 65)`
- By the end of the reveal window: all banner cells are locked, all non-banner cells are gone

### Phase 3 — Hold (3000–3800ms)

Logo sits on screen for 800ms. No animation, just the static banner in matrix green.

### Then

Clear screen, proceed to typewriter text sequence as before.

## Function Signature

`digital_rain(duration_ms)` becomes `digital_rain(duration_ms, reveal: Option<&[&str]>)`.

- When `reveal` is `Some`: pure rain for `duration_ms`, then 1000ms reveal phase, then 800ms hold. Caller provides the banner lines as a slice of string slices.
- When `reveal` is `None`: behavior identical to current implementation (pure rain only).

## Caller Changes

- `matrix_intro()` passes `Some(&REVEAL_BANNER)` — gets rain + reveal + hold
- `matrix_outro()` passes `None` — gets pure rain only (unchanged behavior)

A `REVEAL_BANNER` constant (array of `&str` lines) is defined in `src/tui.rs`.

## Timing Summary

| Phase | Duration | Intro | Outro |
|-------|----------|-------|-------|
| Pure rain | `duration_ms` (2000ms intro, 1500ms outro) | Yes | Yes |
| Reveal | 1000ms | Yes | No |
| Hold | 800ms | Yes | No |
| **Total** | | **3800ms** | **1500ms** |

## Files Changed

### `src/tui.rs`

- Add `REVEAL_BANNER` constant (the banner lines)
- Modify `digital_rain()` signature to accept `reveal: Option<&[&str]>`
- Add reveal phase logic: pre-map banner to grid positions, stop spawning new heads, lock banner cells at random frames, render locked cells in matrix green
- Add hold phase: render static banner for 800ms after reveal
- Update `matrix_intro()` to pass `Some(&REVEAL_BANNER)`
- Update `matrix_outro()` to pass `None`

### No other files change.
