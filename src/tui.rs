use owo_colors::OwoColorize;
use std::io::{self, Write};

// ── Color palette ────────────────────────────────────────────────────────

const WHITE: (u8, u8, u8) = (255, 255, 255);
const DIM: (u8, u8, u8) = (120, 120, 120);
const ROSE: (u8, u8, u8) = (210, 100, 100);

const PHOSPHOR_GREEN: (u8, u8, u8) = (0, 255, 65);
const PHOSPHOR_DIM: (u8, u8, u8) = (0, 140, 30);
const PHOSPHOR_DARK: (u8, u8, u8) = (0, 80, 18);

const fn rgb(color: (u8, u8, u8)) -> owo_colors::Rgb {
    owo_colors::Rgb(color.0, color.1, color.2)
}

// ── Digital rain ─────────────────────────────────────────────────────────

struct RainCell {
    ch: char,
    age: u16,
}

const fn age_to_color(age: u16) -> Option<(u8, u8, u8)> {
    match age {
        0 => Some(WHITE),
        1..=2 => Some((180, 255, 180)),
        3..=5 => Some(PHOSPHOR_GREEN),
        6..=10 => Some((0, 200, 50)),
        11..=16 => Some(PHOSPHOR_DIM),
        17..=24 => Some(PHOSPHOR_DARK),
        _ => None,
    }
}

const fn should_mutate(age: u16, seed: &mut u64) -> bool {
    let roll = (xorshift(seed) % 100) as u16;
    match age {
        0..=2 => roll < 30,
        3..=10 => roll < 15,
        _ => roll < 5,
    }
}

const RAIN_CHARS: &[u8] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz@#$%&*<>{}[]|/\\~";

const fn xorshift(seed: &mut u64) -> u64 {
    if *seed == 0 {
        *seed = 0xDEAD_BEEF_CAFE_1337;
    }
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn random_char(seed: &mut u64) -> char {
    RAIN_CHARS[(xorshift(seed) as usize) % RAIN_CHARS.len()] as char
}

const REVEAL_BANNER: &[&str] = &[
    "\u{2502} \u{2502}\u{2577}\u{2502} \u{2502}\u{2577}\u{2502} \u{2577}  \u{2502}\u{2577}\u{2502} \u{2502}\u{2577}\u{2502} \u{2502}\u{2577}\u{2502}",
    "\u{2502} \u{2575}\u{2502} \u{2502}\u{2575}\u{2502} \u{2575} \u{2577} \u{2575}\u{2502} \u{2502}\u{2575}\u{2502} \u{2502}\u{2575}\u{2502}",
    "\u{2575}  \u{2575} \u{2575} \u{2575}  \u{2502}  \u{2575} \u{2575} \u{2575} \u{2575} \u{2575}",
    "           \u{2575}",
    "      j a c k i n",
    "   operator terminal",
];

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
            if r < rows && c < cols {
                grid[r][c] = Some(ch);
            }
        }
    }
    grid
}

#[allow(clippy::too_many_lines)]
fn digital_rain(duration_ms: u64, reveal: Option<&[&str]>) {
    struct Column {
        head: i32,
        speed: u32,
        active: bool,
        cooldown: u32,
    }

    let cols = 70;
    let rows = 18;
    let frame_ms = 60;
    let total_frames = duration_ms / frame_ms;

    let mut seed: u64 = 0xDEAD_BEEF_CAFE_1337;

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

    let mut grid: Vec<Vec<Option<RainCell>>> = (0..rows)
        .map(|_| (0..cols).map(|_| None).collect())
        .collect();

    eprint!("\x1b[?25l"); // hide cursor

    // ── Phase 1: Pure rain ──────────────────────────────────────────────
    for frame in 0..total_frames {
        // Age all existing cells
        for row in &mut grid {
            for cell in &mut *row {
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

            if frame % u64::from(column.speed) == 0 {
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
        let mut flip_at: Vec<Vec<u64>> =
            (0..rows).map(|_| (0..cols).map(|_| 0).collect()).collect();
        let mut locked: Vec<Vec<bool>> = vec![vec![false; cols]; rows];

        for (r, row) in target.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                if cell.is_some() {
                    flip_at[r][c] = xorshift(&mut seed) % reveal_frames;
                }
            }
        }

        // Stop spawning new heads — deactivate all columns permanently
        for column in &mut columns {
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
                        rc.age += 3;
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
                    if let Some(ch) = target_ch
                        && !locked[r][c]
                        && frame >= flip_at[r][c]
                    {
                        locked[r][c] = true;
                        if *ch == ' ' {
                            grid[r][c] = None;
                        } else {
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

        // Hold the revealed logo briefly
        std::thread::sleep(std::time::Duration::from_millis(1500));
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

// ── Text effects ─────────────────────────────────────────────────────────

fn type_text(text: &str, color: (u8, u8, u8), char_ms: u64) {
    eprint!("  ");
    for ch in text.chars() {
        eprint!("{}", ch.color(rgb(color)));
        let _ = io::stderr().flush();
        std::thread::sleep(std::time::Duration::from_millis(char_ms));
    }
    eprintln!();
}

fn glitch_text(text: &str, color: (u8, u8, u8)) {
    let chars: Vec<char> = text.chars().collect();
    let mut seed: u64 = 0xCAFE_BABE_1337;

    for _ in 0..4 {
        eprint!("\r  ");
        for &ch in &chars {
            let s = xorshift(&mut seed);
            let display = if s.is_multiple_of(4) {
                random_char(&mut seed)
            } else {
                ch
            };
            let (r, g, b) = if s.is_multiple_of(3) {
                PHOSPHOR_GREEN
            } else {
                color
            };
            eprint!("{}", display.color(owo_colors::Rgb(r, g, b)));
        }
        let _ = io::stderr().flush();
        std::thread::sleep(std::time::Duration::from_millis(80));
    }
    eprint!("\r  ");
    eprintln!("{}", text.color(rgb(color)));
}

// ── Matrix intro / outro ─────────────────────────────────────────────────

pub fn matrix_intro(operator_name: &str) {
    clear_screen();

    digital_rain(2000, Some(REVEAL_BANNER));

    clear_screen();
    std::thread::sleep(std::time::Duration::from_millis(300));

    eprintln!();
    type_text(&format!("Wake up, {operator_name}..."), PHOSPHOR_GREEN, 65);
    std::thread::sleep(std::time::Duration::from_millis(800));

    eprintln!();
    type_text("The Matrix has you...", PHOSPHOR_GREEN, 55);
    std::thread::sleep(std::time::Duration::from_millis(600));

    eprintln!();
    type_text("Follow the white rabbit.", PHOSPHOR_GREEN, 50);
    std::thread::sleep(std::time::Duration::from_millis(400));

    eprintln!();
    glitch_text(&format!("Knock, knock, {operator_name}."), PHOSPHOR_GREEN);
    std::thread::sleep(std::time::Duration::from_millis(600));

    clear_screen();
    std::thread::sleep(std::time::Duration::from_millis(200));
}

pub fn matrix_outro(agent_name: &str, remaining: &[String]) {
    clear_screen();

    digital_rain(1500, None);

    clear_screen();
    std::thread::sleep(std::time::Duration::from_millis(300));

    eprintln!();
    type_text(
        &format!("{agent_name} has left the Matrix."),
        PHOSPHOR_GREEN,
        40,
    );
    std::thread::sleep(std::time::Duration::from_millis(400));

    eprintln!();
    if remaining.is_empty() {
        type_text("No agents remain in the Matrix.", PHOSPHOR_DIM, 35);
    } else {
        type_text(
            &format!(
                "{} agent(s) still in the Matrix: {}",
                remaining.len(),
                remaining.join(", ")
            ),
            PHOSPHOR_DIM,
            30,
        );
    }

    std::thread::sleep(std::time::Duration::from_millis(400));
    eprintln!();
    type_text("Connection closed.", PHOSPHOR_DARK, 45);
    std::thread::sleep(std::time::Duration::from_millis(500));
    eprintln!();
}

pub fn simple_outro(agent_name: &str, remaining: &[String]) {
    eprintln!();
    eprintln!(
        "  {}",
        format!("{agent_name} has left the Matrix.").color(rgb(PHOSPHOR_DIM))
    );
    if remaining.is_empty() {
        eprintln!(
            "  {}",
            "No agents remain in the Matrix.".color(rgb(PHOSPHOR_DIM))
        );
    } else {
        eprintln!(
            "  {}",
            format!(
                "{} agent(s) still in the Matrix: {}",
                remaining.len(),
                remaining.join(", ")
            )
            .color(rgb(DIM))
        );
    }
    eprintln!();
}

// ── Config table ─────────────────────────────────────────────────────────

pub fn print_config_table(rows: &[(String, String)]) {
    let label_w = rows.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
    let value_w = rows.iter().map(|(_, v)| v.len()).max().unwrap_or(0);
    let inner_w = label_w + 3 + value_w;

    let dim = rgb(PHOSPHOR_DARK);
    let gold = rgb(PHOSPHOR_GREEN);
    let powder = rgb(PHOSPHOR_DIM);

    eprintln!(
        "  {}{}{}",
        "\u{250c}".color(dim),
        "\u{2500}".repeat(inner_w + 2).color(dim),
        "\u{2510}".color(dim),
    );

    for (label, value) in rows {
        let pad_l = label_w - label.len();
        let pad_r = value_w - value.len();
        eprintln!(
            "  {} {}{} {} {}{}{}",
            "\u{2502}".color(dim),
            " ".repeat(pad_l),
            label.color(gold),
            "\u{2502}".color(dim),
            value.color(powder),
            " ".repeat(pad_r),
            " \u{2502}".to_string().color(dim),
        );
    }

    eprintln!(
        "  {}{}{}",
        "\u{2514}".color(dim),
        "\u{2500}".repeat(inner_w + 2).color(dim),
        "\u{2518}".color(dim),
    );
}

// ── Step shimmer ─────────────────────────────────────────────────────────

pub fn step_shimmer(n: u32, text: &str) {
    let prefix = format!("  {n:>2}.  ");
    let chars: Vec<char> = text.chars().collect();
    let frames = chars.len() + 6;

    let mg = rgb(PHOSPHOR_GREEN);

    for frame in 0..frames {
        eprint!("\r");
        eprint!("{}", prefix.color(mg).bold());
        for (i, ch) in chars.iter().enumerate() {
            let dist = (frame as i32 - i as i32).abs();
            let color = if dist == 0 {
                WHITE
            } else if dist == 1 {
                (150, 255, 170)
            } else if dist == 2 {
                PHOSPHOR_GREEN
            } else {
                PHOSPHOR_DIM
            };
            eprint!("{}", ch.color(rgb(color)).bold());
        }
        let _ = io::stderr().flush();
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    eprint!("\r");
    eprint!("{}", prefix.color(mg).bold());
    eprintln!("{}", text.color(rgb(PHOSPHOR_DIM)).bold());
}

pub fn step_fail(msg: &str) {
    eprintln!("       {}", msg.color(rgb(ROSE)));
}

// ── Deploying message ────────────────────────────────────────────────────

pub fn print_deploying(agent_name: &str) {
    eprintln!();
    eprintln!(
        "  {}",
        format!("Deploying {agent_name} into the Matrix...")
            .color(rgb(PHOSPHOR_GREEN))
            .bold()
    );
    eprintln!();

    std::thread::sleep(std::time::Duration::from_millis(1500));
    clear_screen();
}

// ── Logo ─────────────────────────────────────────────────────────────

pub fn print_logo(logo_path: &std::path::Path) {
    let contents = match std::fs::read_to_string(logo_path) {
        Ok(c) if !c.trim().is_empty() => c,
        _ => return,
    };

    eprintln!();
    for line in contents.lines() {
        eprintln!("  {}", line.color(rgb(PHOSPHOR_GREEN)));
    }
    eprintln!();
}

// ── Utility ──────────────────────────────────────────────────────────────

pub fn fatal(msg: &str) {
    eprintln!();
    eprintln!(
        "  {} {}",
        "error:".color(rgb(ROSE)).bold(),
        msg.color(rgb(ROSE)),
    );
}

pub fn set_terminal_title(title: &str) {
    eprint!("\x1b]0;{title}\x07");
    let _ = io::stderr().flush();
}

pub fn clear_screen() {
    eprint!("\x1b[2J\x1b[H");
    let _ = io::stderr().flush();
}
