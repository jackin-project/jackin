# Rain Logo Reveal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The digital rain intro animation ends by revealing the jackin banner — rain dies while the logo materializes from the noise.

**Architecture:** Single task modifying `digital_rain()` in `src/tui.rs`. Add a `REVEAL_BANNER` constant, change the function signature to accept an optional reveal parameter, add reveal and hold phases after the pure rain phase. Update `matrix_intro` and `matrix_outro` callers.

**Tech Stack:** Rust, owo-colors 4

---

### Task 1: Add logo reveal to digital rain

**Files:**
- Modify: `src/tui.rs:18-170` (digital rain section) and lines 213-244 (matrix_intro/outro callers)

- [ ] **Step 1: Add REVEAL_BANNER constant**

Add after the `random_char` function (after line 61) in `src/tui.rs`:

```rust
const REVEAL_BANNER: &[&str] = &[
    "\u{2502} \u{2502}\u{2577}\u{2502} \u{2502}\u{2577}\u{2502} \u{2577}  \u{2502}\u{2577}\u{2502} \u{2502}\u{2577}\u{2502} \u{2502}\u{2577}\u{2502}",
    "\u{2502} \u{2575}\u{2502} \u{2502}\u{2575}\u{2502} \u{2575} \u{2577} \u{2575}\u{2502} \u{2502}\u{2575}\u{2502} \u{2502}\u{2575}\u{2502}",
    "\u{2575}  \u{2575} \u{2575} \u{2575}  \u{2502}  \u{2575} \u{2575} \u{2575} \u{2575} \u{2575}",
    "           \u{2575}",
    "      j a c k i n",
    "   operator terminal",
];
```

This uses Unicode escapes for the box-drawing characters: `│` = `\u{2502}`, `╷` = `\u{2577}`, `╵` = `\u{2575}`.

- [ ] **Step 2: Add banner_grid helper function**

Add directly after the `REVEAL_BANNER` constant:

```rust
fn banner_grid(banner: &[&str], cols: usize, rows: usize) -> Vec<Vec<Option<char>>> {
    let banner_height = banner.len();
    let banner_width = banner.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let offset_row = (rows.saturating_sub(banner_height)) / 2;
    let offset_col = (cols.saturating_sub(banner_width)) / 2;

    let mut grid = vec![vec![None; cols]; rows];
    for (i, line) in banner.iter().enumerate() {
        for (j, ch) in line.chars().enumerate() {
            let r = offset_row + i;
            let c = offset_col + j;
            if r < rows && c < cols && ch != ' ' {
                grid[r][c] = Some(ch);
            }
        }
    }
    grid
}
```

- [ ] **Step 3: Change digital_rain signature and update callers**

Change the function signature at line 63 from:

```rust
fn digital_rain(duration_ms: u64) {
```

to:

```rust
fn digital_rain(duration_ms: u64, reveal: Option<&[&str]>) {
```

Update `matrix_intro` (line 216) from:

```rust
    digital_rain(2000);
```

to:

```rust
    digital_rain(2000, Some(REVEAL_BANNER));
```

Update `matrix_outro` (line 244) from:

```rust
    digital_rain(1500);
```

to:

```rust
    digital_rain(1500, None);
```

- [ ] **Step 4: Run tests to verify compilation**

Run: `cargo nextest run`
Expected: All 55 tests PASS. The `reveal` parameter is accepted but not yet used — behavior is identical.

- [ ] **Step 5: Commit signature change**

```bash
git add src/tui.rs
git commit -m "refactor: add reveal parameter to digital_rain (no-op for now)"
```

- [ ] **Step 6: Implement reveal and hold phases**

Replace the body of `digital_rain` (everything inside the function, from `let cols = 70;` through the final `flush()`) with the following. This preserves the entire Phase 1 loop and adds Phase 2 (reveal) and Phase 3 (hold) after it:

```rust
fn digital_rain(duration_ms: u64, reveal: Option<&[&str]>) {
    let cols = 70;
    let rows = 18;
    let frame_ms = 60;
    let total_frames = duration_ms / frame_ms;

    let mut seed: u64 = 0xDEAD_BEEF_CAFE_1337;

    struct Column {
        head: i32,
        speed: u32,
        active: bool,
        cooldown: u32,
    }

    let mut columns: Vec<Column> = (0..cols)
        .map(|_| {
            let s = xorshift(&mut seed);
            Column {
                head: -((s % (rows as u64 + 10)) as i32),
                speed: 1 + (s % 3) as u32,
                active: !s.is_multiple_of(3),
                cooldown: 0,
            }
        })
        .collect();

    let mut grid: Vec<Vec<Option<RainCell>>> = (0..rows).map(|_| {
        (0..cols).map(|_| None).collect()
    }).collect();

    eprint!("\x1b[?25l"); // hide cursor

    // ── Phase 1: Pure rain ──────────────────────────────────────────────
    for frame in 0..total_frames {
        // Age all existing cells
        for row in grid.iter_mut() {
            for cell in row.iter_mut() {
                if let Some(c) = cell {
                    c.age += 1;
                    if age_to_color(c.age).is_none() {
                        *cell = None;
                    } else if should_mutate(c.age, &mut seed) {
                        c.ch = random_char(&mut seed);
                    }
                }
            }
        }

        // Advance columns
        for (col, column) in columns.iter_mut().enumerate() {
            if !column.active {
                if column.cooldown > 0 {
                    column.cooldown -= 1;
                } else {
                    column.active = true;
                    column.head = -((xorshift(&mut seed) % 5) as i32);
                    column.speed = 1 + (xorshift(&mut seed) % 3) as u32;
                }
                continue;
            }

            if frame % (column.speed as u64) == 0 {
                column.head += 1;
            }

            let head = column.head;
            if head >= 0 && (head as usize) < rows {
                grid[head as usize][col] = Some(RainCell {
                    ch: random_char(&mut seed),
                    age: 0,
                });
            }

            if head > (rows as i32) + 10 {
                column.active = false;
                column.cooldown = 3 + (xorshift(&mut seed) % 13) as u32;
            }
        }

        // Render
        eprint!("\x1b[H");
        for row in &grid {
            eprint!("  ");
            for cell in row {
                match cell {
                    None => eprint!(" "),
                    Some(c) => {
                        let (r, g, b) = age_to_color(c.age).unwrap_or(PHOSPHOR_DARK);
                        eprint!("{}", c.ch.color(owo_colors::Rgb(r, g, b)));
                    }
                }
            }
            eprintln!();
        }

        let _ = io::stderr().flush();
        std::thread::sleep(std::time::Duration::from_millis(frame_ms));
    }

    // ── Phase 2 & 3: Reveal + Hold (only if reveal banner provided) ─────
    if let Some(banner) = reveal {
        let target = banner_grid(banner, cols, rows);

        // Assign a random flip frame to each banner cell within the reveal window
        let reveal_frames = 1000 / frame_ms;
        let mut flip_at: Vec<Vec<u64>> = (0..rows).map(|_| {
            (0..cols).map(|_| 0).collect()
        }).collect();
        let mut locked: Vec<Vec<bool>> = vec![vec![false; cols]; rows];

        for (r, row) in target.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                if cell.is_some() {
                    flip_at[r][c] = xorshift(&mut seed) % reveal_frames;
                }
            }
        }

        // Stop spawning new heads — deactivate all columns permanently
        for column in columns.iter_mut() {
            column.active = false;
            column.cooldown = u32::MAX;
        }

        // Reveal phase animation
        for frame in 0..reveal_frames {
            // Age existing non-locked cells
            for (r, row) in grid.iter_mut().enumerate() {
                for (c, cell) in row.iter_mut().enumerate() {
                    if locked[r][c] {
                        continue;
                    }
                    if let Some(rc) = cell {
                        rc.age += 1;
                        if age_to_color(rc.age).is_none() {
                            *cell = None;
                        } else if should_mutate(rc.age, &mut seed) {
                            rc.ch = random_char(&mut seed);
                        }
                    }
                }
            }

            // Lock banner cells that have reached their flip frame
            for (r, row) in target.iter().enumerate() {
                for (c, target_ch) in row.iter().enumerate() {
                    if let Some(ch) = target_ch {
                        if !locked[r][c] && frame >= flip_at[r][c] {
                            locked[r][c] = true;
                            grid[r][c] = Some(RainCell { ch: *ch, age: 0 });
                        }
                    }
                }
            }

            // Render
            eprint!("\x1b[H");
            for (r, row) in grid.iter().enumerate() {
                eprint!("  ");
                for (c, cell) in row.iter().enumerate() {
                    if locked[r][c] {
                        if let Some(rc) = cell {
                            eprint!("{}", rc.ch.color(rgb(PHOSPHOR_GREEN)));
                        } else {
                            eprint!(" ");
                        }
                    } else {
                        match cell {
                            None => eprint!(" "),
                            Some(rc) => {
                                let (cr, cg, cb) = age_to_color(rc.age).unwrap_or(PHOSPHOR_DARK);
                                eprint!("{}", rc.ch.color(owo_colors::Rgb(cr, cg, cb)));
                            }
                        }
                    }
                }
                eprintln!();
            }

            let _ = io::stderr().flush();
            std::thread::sleep(std::time::Duration::from_millis(frame_ms));
        }

        // ── Phase 3: Hold ───────────────────────────────────────────────
        std::thread::sleep(std::time::Duration::from_millis(800));
    }

    // Clear rain area
    eprint!("\x1b[H");
    for _ in 0..rows {
        eprintln!("  {:width$}", "", width = cols);
    }
    eprint!("\x1b[H");
    eprint!("\x1b[?25h"); // show cursor
    let _ = io::stderr().flush();
}
```

- [ ] **Step 7: Run all tests**

Run: `cargo nextest run`
Expected: All 55 tests PASS.

Run: `cargo clippy -- -D warnings`
Expected: No warnings.

- [ ] **Step 8: Commit**

```bash
git add src/tui.rs
git commit -m "feat: add logo reveal phase to digital rain intro animation"
```
