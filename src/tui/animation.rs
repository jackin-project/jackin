use owo_colors::OwoColorize;
use std::io::{self, Write};

use super::{PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE, clear_screen, rgb};

// ── Color palette ────────────────────────────────────────────────────────

const DIM: (u8, u8, u8) = (120, 120, 120);

// ── Skippable sleep ─────────────────────────────────────────────────────

/// Sleep for `duration`, but return `true` immediately if Enter or Esc is pressed.
/// Temporarily enables raw mode for keypress detection, then restores it.
fn skippable_sleep(duration: std::time::Duration) -> bool {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};

    let _ = crossterm::terminal::enable_raw_mode();
    let skipped = if crossterm::event::poll(duration).unwrap_or(false) {
        matches!(
            event::read(),
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press
                && matches!(key.code, KeyCode::Enter | KeyCode::Esc)
        )
    } else {
        false
    };
    let _ = crossterm::terminal::disable_raw_mode();
    skipped
}

// ── Digital rain ─────────────────────────────────────────────────────────

pub(crate) struct RainCell {
    pub(crate) ch: char,
    pub(crate) age: u16,
    /// How many age units to add per frame (1 = long trail, 3 = short trail).
    pub(crate) fade: u16,
}

pub(crate) struct RainColumn {
    pub(crate) head: i32,
    pub(crate) speed: u32,
    /// Fade rate for cells deposited by this column (1 = long, 3 = short).
    pub(crate) fade: u16,
    pub(crate) active: bool,
    pub(crate) cooldown: u32,
}

pub(crate) struct RainState {
    pub(crate) grid: Vec<Vec<Option<RainCell>>>,
    pub(crate) columns: Vec<RainColumn>,
    pub(crate) cols: usize,
    pub(crate) rows: usize,
    pub(crate) seed: u64,
    pub(crate) frame: u64,
}

impl RainState {
    pub(crate) fn new(cols: usize, rows: usize) -> Self {
        let mut seed: u64 = 0xDEAD_BEEF_CAFE_1337;

        let columns: Vec<RainColumn> = (0..cols)
            .map(|_| {
                let s = xorshift(&mut seed);
                let s2 = xorshift(&mut seed);
                RainColumn {
                    head: -((s % (rows as u64 + 6)) as i32),
                    speed: 1 + (s % 4) as u32,
                    fade: 1 + (s2 % 3) as u16,
                    active: !s.is_multiple_of(3),
                    cooldown: 0,
                }
            })
            .collect();

        let grid: Vec<Vec<Option<RainCell>>> =
            (0..rows).map(|_| (0..cols).map(|_| None).collect()).collect();

        Self {
            grid,
            columns,
            cols,
            rows,
            seed,
            frame: 0,
        }
    }
}

pub(crate) const fn age_to_color(age: u16) -> Option<(u8, u8, u8)> {
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

pub(crate) const fn xorshift(seed: &mut u64) -> u64 {
    if *seed == 0 {
        *seed = 0xDEAD_BEEF_CAFE_1337;
    }
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

pub(crate) fn random_char(seed: &mut u64) -> char {
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

/// Advance the rain state by one tick: age existing cells and move column
/// heads forward. This is the simulation step; call `render_rain_frame`
/// afterward to draw the result.
pub(crate) fn tick_rain(state: &mut RainState) {
    let RainState {
        grid,
        columns,
        rows,
        seed,
        frame,
        ..
    } = state;

    // Age all existing cells (each cell fades at its own rate)
    for row in &mut *grid {
        for cell in &mut *row {
            if let Some(c) = cell {
                c.age += c.fade;
                if age_to_color(c.age).is_none() {
                    *cell = None;
                } else if should_mutate(c.age, seed) {
                    c.ch = random_char(seed);
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
                column.head = -((xorshift(seed) % 6) as i32);
                column.speed = 1 + (xorshift(seed) % 4) as u32;
                column.fade = 1 + (xorshift(seed) % 3) as u16;
            }
            continue;
        }

        if *frame % u64::from(column.speed) == 0 {
            column.head += 1;
        }

        let head = column.head;
        if head >= 0 && (head as usize) < *rows {
            grid[head as usize][col] = Some(RainCell {
                ch: random_char(seed),
                age: 0,
                fade: column.fade,
            });
        }

        if head > (*rows as i32) + 5 {
            column.active = false;
            column.cooldown = 2 + (xorshift(seed) % 18) as u32;
        }
    }

    *frame += 1;
}

/// Render a single frame of digital rain into a bounded area.
/// Used by `digital_rain` (fullscreen) and by the panel-rain widget
/// (area-bounded). Does not clear the background — callers that need
/// a clear should emit it before calling this.
pub(crate) fn render_rain_frame(state: &mut RainState, area: (u16, u16, u16, u16)) {
    let (col_start, row_start, width, height) = area;

    for r in 0..height as usize {
        eprint!("\x1b[{};{}H", row_start as usize + r + 1, col_start + 1);
        for c in 0..width as usize {
            match state.grid.get(r).and_then(|row| row.get(c)) {
                None | Some(None) => eprint!(" "),
                Some(Some(cell)) => match age_to_color(cell.age) {
                    None => eprint!(" "),
                    Some((red, g, b)) => {
                        eprint!("{}", cell.ch.color(owo_colors::Rgb(red, g, b)));
                    }
                },
            }
        }
    }

    let _ = io::stderr().flush();
}

#[allow(clippy::too_many_lines)]
pub(crate) fn digital_rain(duration_ms: u64, reveal: Option<&[&str]>) {
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let cols = term_cols as usize;
    // Reserve last row to avoid scroll when writing to it
    let rows = (term_rows as usize).saturating_sub(1).max(1);
    let frame_ms = 35;
    let total_frames = duration_ms / frame_ms;

    let mut seed: u64 = 0xDEAD_BEEF_CAFE_1337;

    let columns: Vec<RainColumn> = (0..cols)
        .map(|_| {
            let s = xorshift(&mut seed);
            let s2 = xorshift(&mut seed);
            RainColumn {
                head: -((s % (rows as u64 + 6)) as i32),
                speed: 1 + (s % 4) as u32,
                fade: 1 + (s2 % 3) as u16,
                active: !s.is_multiple_of(3),
                cooldown: 0,
            }
        })
        .collect();

    let grid: Vec<Vec<Option<RainCell>>> = (0..rows)
        .map(|_| (0..cols).map(|_| None).collect())
        .collect();

    let mut state = RainState {
        grid,
        columns,
        cols,
        rows,
        seed,
        frame: 0,
    };

    eprint!("\x1b[?25l"); // hide cursor

    // ── Phase 1: Pure rain ──────────────────────────────────────────────
    let mut skipped = false;
    for _ in 0..total_frames {
        if skipped {
            break;
        }
        tick_rain(&mut state);
        render_rain_frame(&mut state, (0, 0, cols as u16, rows as u16));
        skipped = skippable_sleep(std::time::Duration::from_millis(frame_ms));
    }

    // Sync seed back for reveal phase (seed was updated inside state)
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
                    flip_at[r][c] = xorshift(&mut state.seed) % reveal_frames;
                }
            }
        }

        // Stop spawning new heads — deactivate all columns permanently
        for column in &mut state.columns {
            column.active = false;
            column.cooldown = u32::MAX;
        }

        // Reveal phase animation
        for frame in 0..reveal_frames {
            if skipped {
                break;
            }
            // Age existing non-locked cells
            for (r, row) in state.grid.iter_mut().enumerate() {
                for (c, cell) in row.iter_mut().enumerate() {
                    if locked[r][c] {
                        continue;
                    }
                    if let Some(rc) = cell {
                        rc.age += 3;
                        if age_to_color(rc.age).is_none() {
                            *cell = None;
                        } else if should_mutate(rc.age, &mut state.seed) {
                            rc.ch = random_char(&mut state.seed);
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
                            state.grid[r][c] = None;
                        } else {
                            state.grid[r][c] = Some(RainCell {
                                ch: *ch,
                                age: 0,
                                fade: 1,
                            });
                        }
                    }
                }
            }

            // Render
            for (r, row) in state.grid.iter().enumerate() {
                eprint!("\x1b[{};1H", r + 1);
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
            }

            let _ = io::stderr().flush();
            skipped = skippable_sleep(std::time::Duration::from_millis(frame_ms));
        }

        // Hold the revealed logo briefly
        if !skipped {
            let _ = skippable_sleep(std::time::Duration::from_millis(1500));
        }
    }

    // Clear rain area
    for r in 0..rows {
        eprint!("\x1b[{};1H\x1b[2K", r + 1);
    }
    eprint!("\x1b[H");
    eprint!("\x1b[?25h"); // show cursor
    let _ = io::stderr().flush();
}

// ── Text effects ─────────────────────────────────────────────────────────

/// Returns `true` if skipped by keypress.
fn type_text(text: &str, color: (u8, u8, u8), char_ms: u64) -> bool {
    eprint!("  ");
    for ch in text.chars() {
        eprint!("{}", ch.color(rgb(color)));
        let _ = io::stderr().flush();
        if skippable_sleep(std::time::Duration::from_millis(char_ms)) {
            // Print remainder instantly
            eprintln!();
            return true;
        }
    }
    eprintln!();
    false
}

/// Returns `true` if skipped by keypress.
fn glitch_text(text: &str, color: (u8, u8, u8)) -> bool {
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
        if skippable_sleep(std::time::Duration::from_millis(80)) {
            eprint!("\r  ");
            eprintln!("{}", text.color(rgb(color)));
            return true;
        }
    }
    eprint!("\r  ");
    eprintln!("{}", text.color(rgb(color)));
    false
}

// ── Intro / outro animation ──────────────────────────────────────────────

pub fn intro_animation(operator_name: &str) {
    clear_screen();

    digital_rain(2000, Some(REVEAL_BANNER));

    clear_screen();
    if skippable_sleep(std::time::Duration::from_millis(300)) {
        return;
    }

    eprintln!();
    if type_text(&format!("Stand up, {operator_name}..."), PHOSPHOR_GREEN, 65) {
        clear_screen();
        return;
    }
    if skippable_sleep(std::time::Duration::from_millis(800)) {
        clear_screen();
        return;
    }

    eprintln!();
    if type_text("They're already inside...", PHOSPHOR_GREEN, 55) {
        clear_screen();
        return;
    }
    if skippable_sleep(std::time::Duration::from_millis(600)) {
        clear_screen();
        return;
    }

    eprintln!();
    if type_text("Follow the green.", PHOSPHOR_GREEN, 50) {
        clear_screen();
        return;
    }
    if skippable_sleep(std::time::Duration::from_millis(400)) {
        clear_screen();
        return;
    }

    eprintln!();
    glitch_text(&format!("Knock, knock, {operator_name}."), PHOSPHOR_GREEN);
    if skippable_sleep(std::time::Duration::from_millis(600)) {
        clear_screen();
        return;
    }

    clear_screen();
    let _ = skippable_sleep(std::time::Duration::from_millis(200));
}

pub fn outro_animation(agent_name: &str, remaining: &[String]) {
    clear_screen();

    digital_rain(1500, None);

    clear_screen();
    if skippable_sleep(std::time::Duration::from_millis(300)) {
        return;
    }

    eprintln!();
    if type_text(
        &format!("{agent_name} has left the container."),
        PHOSPHOR_GREEN,
        40,
    ) {
        eprintln!();
        return;
    }
    if skippable_sleep(std::time::Duration::from_millis(400)) {
        eprintln!();
        return;
    }

    eprintln!();
    let skipped = if remaining.is_empty() {
        type_text("No agents remaining.", PHOSPHOR_DIM, 35)
    } else {
        type_text(
            &format!(
                "{} agent(s) still running: {}",
                remaining.len(),
                remaining.join(", ")
            ),
            PHOSPHOR_DIM,
            30,
        )
    };
    if skipped {
        eprintln!();
        return;
    }

    if skippable_sleep(std::time::Duration::from_millis(400)) {
        eprintln!();
        return;
    }
    eprintln!();
    type_text("Connection closed.", PHOSPHOR_DARK, 45);
    let _ = skippable_sleep(std::time::Duration::from_millis(500));
    eprintln!();
}

pub fn simple_outro(agent_name: &str, remaining: &[String]) {
    eprintln!();
    eprintln!(
        "  {}",
        format!("{agent_name} has left the container.").color(rgb(PHOSPHOR_DIM))
    );
    if remaining.is_empty() {
        eprintln!("  {}", "No agents remaining.".color(rgb(PHOSPHOR_DIM)));
    } else {
        eprintln!(
            "  {}",
            format!(
                "{} agent(s) still running: {}",
                remaining.len(),
                remaining.join(", ")
            )
            .color(rgb(DIM))
        );
    }
    eprintln!();
}
