use owo_colors::OwoColorize;
use std::io::{self, Write};

// ── Color palette ────────────────────────────────────────────────────────

const WHITE: (u8, u8, u8) = (255, 255, 255);
const DIM: (u8, u8, u8) = (120, 120, 120);
const ROSE: (u8, u8, u8) = (210, 100, 100);

const MATRIX_GREEN: (u8, u8, u8) = (0, 255, 65);
const MATRIX_DIM: (u8, u8, u8) = (0, 140, 30);
const MATRIX_DARK: (u8, u8, u8) = (0, 80, 18);

fn rgb(color: (u8, u8, u8)) -> owo_colors::Rgb {
    owo_colors::Rgb(color.0, color.1, color.2)
}

// ── Digital rain ─────────────────────────────────────────────────────────

const RAIN_CHARS: &[u8] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz@#$%&*<>{}[]|/\\~";

fn xorshift(seed: &mut u64) -> u64 {
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

fn digital_rain(duration_ms: u64) {
    let cols = 70;
    let rows = 18;
    let frame_ms = 60;
    let total_frames = duration_ms / frame_ms;

    let mut seed: u64 = 0xDEAD_BEEF_CAFE_1337;

    let mut drops: Vec<(i32, i32, bool)> = (0..cols)
        .map(|_| {
            let s = xorshift(&mut seed);
            let speed = 1 + (s % 3) as i32;
            let start = -((s % (rows as u64 + 5)) as i32);
            (start, speed, (s % 3) != 0)
        })
        .collect();

    let mut grid = vec![vec![' '; cols]; rows];

    eprint!("\x1b[?25l"); // hide cursor

    for frame in 0..total_frames {
        for (col, (pos, speed, active)) in drops.iter_mut().enumerate() {
            if !*active {
                let s = xorshift(&mut seed);
                if s % 8 == 0 {
                    *active = true;
                    *pos = -((s % 5) as i32);
                    *speed = 1 + (s % 3) as i32;
                }
                continue;
            }

            if frame % (*speed as u64) == 0 {
                *pos += 1;
            }

            let head = *pos;
            if head >= 0 && (head as usize) < rows {
                grid[head as usize][col] = random_char(&mut seed);
            }

            let tail = head - 8;
            if tail >= 0 && (tail as usize) < rows {
                grid[tail as usize][col] = ' ';
            }

            let s = xorshift(&mut seed);
            if s % 5 == 0 {
                let r = (s as usize) % rows;
                if grid[r][col] != ' ' {
                    grid[r][col] = random_char(&mut seed);
                }
            }

            if tail > rows as i32 {
                *active = false;
                *pos = -((xorshift(&mut seed) % 8) as i32) - 3;
            }
        }

        eprint!("\x1b[H");
        for row in &grid {
            eprint!("  ");
            for (col_idx, &ch) in row.iter().enumerate() {
                if ch == ' ' {
                    eprint!(" ");
                    continue;
                }
                let head_pos = drops[col_idx].0;
                let dist = head_pos - col_idx as i32;
                // Approximate row distance for coloring
                let color = if dist == 0 {
                    WHITE
                } else if dist == 1 {
                    MATRIX_GREEN
                } else if dist < 4 {
                    MATRIX_DIM
                } else {
                    MATRIX_DARK
                };
                eprint!("{}", ch.color(rgb(color)));
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
            let display = if s % 4 == 0 {
                random_char(&mut seed)
            } else {
                ch
            };
            let (r, g, b) = if s % 3 == 0 {
                MATRIX_GREEN
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

    digital_rain(2000);

    clear_screen();
    std::thread::sleep(std::time::Duration::from_millis(300));

    eprintln!();
    type_text(&format!("Wake up, {operator_name}..."), MATRIX_GREEN, 65);
    std::thread::sleep(std::time::Duration::from_millis(800));

    eprintln!();
    type_text("The Matrix has you...", MATRIX_GREEN, 55);
    std::thread::sleep(std::time::Duration::from_millis(600));

    eprintln!();
    type_text("Follow the white rabbit.", MATRIX_GREEN, 50);
    std::thread::sleep(std::time::Duration::from_millis(400));

    eprintln!();
    glitch_text(&format!("Knock, knock, {operator_name}."), MATRIX_GREEN);
    std::thread::sleep(std::time::Duration::from_millis(600));

    clear_screen();
    std::thread::sleep(std::time::Duration::from_millis(200));
}

pub fn matrix_outro(agent_name: &str, remaining: &[String]) {
    clear_screen();

    digital_rain(1500);

    clear_screen();
    std::thread::sleep(std::time::Duration::from_millis(300));

    eprintln!();
    type_text(
        &format!("{agent_name} has left the Matrix."),
        MATRIX_GREEN,
        40,
    );
    std::thread::sleep(std::time::Duration::from_millis(400));

    eprintln!();
    if remaining.is_empty() {
        type_text("No agents remain in the Matrix.", MATRIX_DIM, 35);
    } else {
        type_text(
            &format!(
                "{} agent(s) still in the Matrix: {}",
                remaining.len(),
                remaining.join(", ")
            ),
            MATRIX_DIM,
            30,
        );
    }

    std::thread::sleep(std::time::Duration::from_millis(400));
    eprintln!();
    type_text("Connection closed.", MATRIX_DARK, 45);
    std::thread::sleep(std::time::Duration::from_millis(500));
    eprintln!();
}

pub fn simple_outro(agent_name: &str, remaining: &[String]) {
    eprintln!();
    eprintln!(
        "  {}",
        format!("{agent_name} has left the Matrix.").color(rgb(MATRIX_DIM))
    );
    if remaining.is_empty() {
        eprintln!(
            "  {}",
            "No agents remain in the Matrix.".color(rgb(MATRIX_DIM))
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

    let dim = rgb(MATRIX_DARK);
    let gold = rgb(MATRIX_GREEN);
    let powder = rgb(MATRIX_DIM);

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
            format!(" \u{2502}").color(dim),
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
    let prefix = format!("  {:>2}.  ", n);
    let chars: Vec<char> = text.chars().collect();
    let frames = chars.len() + 6;

    let mg = rgb(MATRIX_GREEN);

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
                MATRIX_GREEN
            } else {
                MATRIX_DIM
            };
            eprint!("{}", ch.color(rgb(color)).bold());
        }
        let _ = io::stderr().flush();
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    eprint!("\r");
    eprint!("{}", prefix.color(mg).bold());
    eprintln!("{}", text.color(rgb(MATRIX_DIM)).bold());
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
            .color(rgb(MATRIX_GREEN))
            .bold()
    );
    eprintln!();

    std::thread::sleep(std::time::Duration::from_millis(800));
    clear_screen();
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
