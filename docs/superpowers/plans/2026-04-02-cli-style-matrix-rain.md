# Matrix-Styled CLI & Cinematic Rain Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Matrix-themed CLI help styling with rain/jack banner and rewrite digital rain with per-cell age tracking for cinematic visuals.

**Architecture:** Two independent changes in two files. Task 1 adds clap styles and banner to `src/cli.rs`. Task 2 rewrites `digital_rain()` in `src/tui.rs` with a per-cell age model. No new dependencies.

**Tech Stack:** Rust, clap 4 (derive + color features), owo-colors 4

---

### Task 1: Add Matrix-styled CLI help with banner

**Files:**
- Modify: `src/cli.rs:1-76`

- [ ] **Step 1: Write test for styled help output**

Add to the existing `mod tests` block in `src/cli.rs`:

```rust
#[test]
fn help_contains_banner_and_matrix_descriptions() {
    let err = Cli::try_parse_from(["jackin", "--help"]).unwrap_err();
    let help = err.to_string();
    assert!(help.contains("j a c k i n"), "banner missing");
    assert!(help.contains("operator terminal"), "banner tagline missing");
    assert!(help.contains("Send agents into the Matrix"), "about text missing");
}

#[test]
fn load_help_contains_matrix_description() {
    let err = Cli::try_parse_from(["jackin", "load", "--help"]).unwrap_err();
    let help = err.to_string();
    assert!(help.contains("Jack an agent into the Matrix"), "load description missing");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -E 'test(cli::tests::help_contains_banner)' -E 'test(cli::tests::load_help)'`
Expected: FAIL — banner and new descriptions not yet present.

- [ ] **Step 3: Add HELP_STYLES constant and BANNER constant**

At the top of `src/cli.rs`, add the imports and constants before the `Cli` struct:

```rust
use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand};

const HELP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Green.on_default())
    .valid(AnsiColor::BrightGreen.on_default())
    .invalid(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD));

const BANNER: &str = r#"
    │ │╷│ │╷│ ╷  │╷│ │╷│ │╷│
    │ ╵│ │╵│ ╵ ╷ ╵│ │╵│ │╵│
    ╵  ╵ ╵ ╵  │  ╵ ╵ ╵ ╵ ╵
               ╵
          j a c k i n
       operator terminal
"#;
```

- [ ] **Step 4: Apply styles, banner, and Matrix descriptions to Cli and all commands**

Replace the entire `Cli` struct and `Command` enum with:

```rust
/// Send agents into the Matrix
#[derive(Debug, Parser)]
#[command(name = "jackin", version, styles = HELP_STYLES, before_help = BANNER)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Jack an agent into the Matrix
    Load {
        /// Agent class selector (e.g. agent-smith, chainargos/agent-brown)
        selector: String,
        /// Bypass the construct sequence
        #[arg(long, default_value_t = false)]
        no_intro: bool,
        /// Show raw signal output
        #[arg(long, default_value_t = false)]
        debug: bool,
    },
    /// Reattach to a running agent
    Hardline {
        /// Container name to reattach to
        container: String,
    },
    /// Pull an agent out of the Matrix
    Eject {
        /// Agent class selector or container name
        selector: String,
        /// Pull every instance of this class
        #[arg(long)]
        all: bool,
        /// Delete persisted state after ejection
        #[arg(long)]
        purge: bool,
    },
    /// Pull every agent out
    Exile,
    /// Delete persisted state for an agent class
    Purge {
        /// Agent class selector
        selector: String,
        /// Delete state for every instance of this class
        #[arg(long)]
        all: bool,
    },
    /// Operator configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}
```

The `ConfigCommand` and `MountCommand` enums stay unchanged — their descriptions are functional, not thematic.

- [ ] **Step 5: Run all tests to verify they pass**

Run: `cargo nextest run -E 'test(cli::tests)'`
Expected: All 6 tests PASS (4 existing + 2 new).

- [ ] **Step 6: Manually verify help output looks correct**

Run: `cargo run -- --help`
Expected: Banner displays above usage, headers and commands in green, Matrix-flavored descriptions visible.

Run: `cargo run -- load --help`
Expected: "Jack an agent into the Matrix" as description, args described with Matrix flavor.

- [ ] **Step 7: Commit**

```bash
git add src/cli.rs
git commit -m "feat: add Matrix-themed CLI help styling with rain/jack banner"
```

---

### Task 2: Rewrite digital rain with per-cell age tracking

**Files:**
- Modify: `src/tui.rs:18-135` (the `digital_rain` function and supporting code)

- [ ] **Step 1: Add RainCell struct and age_to_color helper**

Add after the `rgb` function (after line 16) in `src/tui.rs`:

```rust
struct RainCell {
    ch: char,
    age: u16,
}

fn age_to_color(age: u16) -> Option<(u8, u8, u8)> {
    match age {
        0 => Some(WHITE),
        1..=2 => Some((180, 255, 180)),
        3..=5 => Some(MATRIX_GREEN),
        6..=10 => Some((0, 200, 50)),
        11..=16 => Some(MATRIX_DIM),
        17..=24 => Some(MATRIX_DARK),
        _ => None,
    }
}
```

- [ ] **Step 2: Add mutation_probability helper**

Add directly after `age_to_color`:

```rust
fn should_mutate(age: u16, seed: &mut u64) -> bool {
    let roll = (xorshift(seed) % 100) as u16;
    match age {
        0..=2 => roll < 30,
        3..=10 => roll < 15,
        _ => roll < 5,
    }
}
```

- [ ] **Step 3: Rewrite digital_rain function**

Replace the entire `digital_rain` function (lines 37-135) with:

```rust
fn digital_rain(duration_ms: u64) {
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
                active: s % 3 != 0,
                cooldown: 0,
            }
        })
        .collect();

    let mut grid: Vec<Vec<Option<RainCell>>> = (0..rows).map(|_| {
        (0..cols).map(|_| None).collect()
    }).collect();

    eprint!("\x1b[?25l"); // hide cursor

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
                        let (r, g, b) = age_to_color(c.age).unwrap_or(MATRIX_DARK);
                        eprint!("{}", c.ch.color(owo_colors::Rgb(r, g, b)));
                    }
                }
            }
            eprintln!();
        }

        let _ = io::stderr().flush();
        std::thread::sleep(std::time::Duration::from_millis(frame_ms));
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

- [ ] **Step 4: Run all tests to verify nothing broke**

Run: `cargo nextest run`
Expected: All tests PASS. The `digital_rain` function is not directly tested (visual output), but compilation and no regressions in other tests confirms correctness.

- [ ] **Step 5: Manually verify the rain animation**

Run: `cargo run -- load agent-smith`
Expected: Rain animation plays for ~2 seconds with:
- Bright white heads
- Smooth 7-step color gradient trailing behind
- Characters flickering/mutating in the trail
- Columns restarting at staggered intervals
- Natural-looking density variation

Press Ctrl+C after the animation to exit.

- [ ] **Step 6: Commit**

```bash
git add src/tui.rs
git commit -m "feat: rewrite digital rain with per-cell age tracking for cinematic visuals"
```
