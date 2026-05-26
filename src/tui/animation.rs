use owo_colors::OwoColorize;
use std::io::{self, Write};

use super::{PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE, clear_screen, rgb};

// ── Skippable sleep ─────────────────────────────────────────────────────

/// Sleep for `duration`, but return `true` immediately if Enter or Esc is pressed.
/// Temporarily enables raw mode for keypress detection, then restores it.
fn skippable_sleep(duration: std::time::Duration) -> bool {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};

    // Under the host guard raw mode is already on for the whole flow; toggling
    // it here would hand control back to the cooked terminal mid-animation.
    let owns_raw = !crate::tui::host_screen_owned();
    if owns_raw {
        let _ = crossterm::terminal::enable_raw_mode();
    }
    let skipped = if crossterm::event::poll(duration).unwrap_or(false) {
        matches!(
            event::read(),
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press
                && matches!(key.code, KeyCode::Enter | KeyCode::Esc)
        )
    } else {
        false
    };
    if owns_raw {
        let _ = crossterm::terminal::disable_raw_mode();
    }
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

        let grid: Vec<Vec<Option<RainCell>>> = (0..rows)
            .map(|_| (0..cols).map(|_| None).collect())
            .collect();

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

pub(crate) const REVEAL_BANNER: &[&str] = &[
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
pub(crate) fn render_rain_frame(state: &RainState, area: (u16, u16, u16, u16)) {
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
        render_rain_frame(&state, (0, 0, cols as u16, rows as u16));
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


// ── Session rain + logo ──────────────────────────────────────────────────

/// Entry ritual: fast rain resolving into the logo, tagged "entering the
/// Construct", with a "start your day" phrase at the bottom. Played once when
/// the console opens.
pub fn rain_logo_intro() {
    rain_logo_with(
        "entering the Construct",
        super::quotes::pick(super::quotes::START_QUOTES),
        None,
    );
}

/// Exit ritual: fast rain resolving into the logo, played when the last
/// container leaves.
///
/// Tagged "leaving the Construct", with a "wind down" phrase and — when known —
/// how long the operator was in the Construct.
pub fn rain_logo_outro(elapsed: Option<std::time::Duration>) {
    let footer = elapsed.map(|d| format!("in the Construct for {}", format_universe_duration(d)));
    rain_logo_with(
        "leaving the Construct",
        super::quotes::pick(super::quotes::END_QUOTES),
        footer.as_deref(),
    );
}

/// Fast digital rain that resolves into the jackin' logo, then holds.
///
/// While holding it shows a tagline beneath the logo, the phrase of the day at
/// the bottom, and an optional footer line, then clears. No prose body — just
/// rain, logo, and captions.
fn rain_logo_with(tagline: &str, quote: Option<&super::quotes::Quote>, footer: Option<&str>) {
    clear_screen();
    // Brief, brisk rainfall that reveals the logo (the reveal + hold happen
    // inside digital_rain when a banner is supplied).
    digital_rain(900, Some(REVEAL_BANNER));
    print_logo_caption(tagline, quote, footer);
    // Linger so the quote is readable, then wipe so the next surface — the
    // console manager on entry, or the shell on exit — starts clean.
    let _ = skippable_sleep(std::time::Duration::from_millis(1900));
    clear_screen();
}

/// Render the "phrase of the day" anchored to the bottom of the logo screen,
/// centered and bright/white so it reads, with the author and any footer line
/// (e.g. time in the construct) dimmer beneath it. Uses absolute cursor moves
/// like `digital_rain`. The logo stays centered; this fills the lower margin.
fn print_logo_caption(tagline: &str, quote: Option<&super::quotes::Quote>, footer: Option<&str>) {
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let cols = term_cols as usize;
    let truncate = |s: &str| -> String {
        let max = cols.saturating_sub(4).max(8);
        if s.chars().count() > max {
            let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
            t.push('\u{2026}');
            t
        } else {
            s.to_string()
        }
    };
    let center = |row: u16, text: &str, color: (u8, u8, u8)| {
        if row == 0 || row > term_rows {
            return;
        }
        let t = truncate(text);
        let col = (cols.saturating_sub(t.chars().count()) / 2).max(1);
        eprint!("\x1b[{row};{col}H{}", t.color(rgb(color)));
    };
    // Tagline ("entering / leaving the Construct") one blank row below the
    // vertically-centered logo (see `banner_grid`).
    if !tagline.is_empty() {
        let logo_top = (term_rows as usize).saturating_sub(REVEAL_BANNER.len()) / 2;
        let row = u16::try_from(logo_top + REVEAL_BANNER.len() + 1).unwrap_or(term_rows);
        center(row, tagline, PHOSPHOR_GREEN);
    }
    // Anchor to the bottom: the footer (if any) on the lowest line, the author
    // above it, the quote above that — leaving the very last row as margin.
    let mut row = term_rows.saturating_sub(1);
    if let Some(f) = footer {
        center(row, f, PHOSPHOR_DIM);
        row = row.saturating_sub(1);
    }
    if let Some(q) = quote {
        center(row, &format!("\u{2014} {}", q.author), PHOSPHOR_DIM);
        center(
            row.saturating_sub(1),
            &format!("\u{201C}{}\u{201D}", q.text),
            WHITE,
        );
    }
    let _ = io::stderr().flush();
}

/// Format a session duration compactly: `2h 14m`, `7m 30s`, or `45s`.
#[must_use]
pub fn format_universe_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}

#[cfg(test)]
mod tests {
    use super::format_universe_duration;
    use std::time::Duration;

    #[test]
    fn formats_session_duration_compactly() {
        assert_eq!(format_universe_duration(Duration::from_secs(45)), "45s");
        assert_eq!(format_universe_duration(Duration::from_secs(450)), "7m 30s");
        assert_eq!(format_universe_duration(Duration::from_secs(8040)), "2h 14m");
        assert_eq!(format_universe_duration(Duration::from_secs(0)), "0s");
    }
}
