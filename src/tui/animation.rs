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

/// Outcome of a resize-aware wait.
enum WaitOutcome {
    /// The full duration elapsed with no interruption.
    Elapsed,
    /// The operator pressed Enter/Esc to skip.
    Skipped,
    /// The terminal was resized; the caller should redraw at the new size.
    Resized,
}

/// Wait up to `duration`, returning early on a skip key (Enter/Esc) or a
/// terminal resize. Same raw-mode handling as `skippable_sleep`. Non-skip,
/// non-resize events (stray mouse, focus) are consumed without ending the wait.
fn wait_or_event(duration: std::time::Duration) -> WaitOutcome {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    let owns_raw = !crate::tui::host_screen_owned();
    if owns_raw {
        let _ = crossterm::terminal::enable_raw_mode();
    }
    let deadline = std::time::Instant::now() + duration;
    let outcome = loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break WaitOutcome::Elapsed;
        }
        if event::poll(remaining).unwrap_or(false) {
            match event::read() {
                Ok(Event::Key(k))
                    if k.kind == KeyEventKind::Press
                        && matches!(k.code, KeyCode::Enter | KeyCode::Esc) =>
                {
                    break WaitOutcome::Skipped;
                }
                Ok(Event::Resize(_, _)) => break WaitOutcome::Resized,
                Ok(_) => {}
                Err(_) => break WaitOutcome::Elapsed,
            }
        } else {
            break WaitOutcome::Elapsed;
        }
    };
    if owns_raw {
        let _ = crossterm::terminal::disable_raw_mode();
    }
    outcome
}

/// Show a static screen for `total`, calling `draw` once up front and again
/// (after a clear) on every terminal resize so the surface always fills and
/// centers to the current size. Returns `true` if the operator skipped.
fn hold_resizable(total: std::time::Duration, mut draw: impl FnMut()) -> bool {
    draw();
    let _ = io::stderr().flush();
    let deadline = std::time::Instant::now() + total;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return false;
        }
        match wait_or_event(remaining) {
            WaitOutcome::Skipped => return true,
            WaitOutcome::Resized => {
                clear_screen();
                draw();
                let _ = io::stderr().flush();
            }
            WaitOutcome::Elapsed => return false,
        }
    }
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
    use std::fmt::Write as _;
    let (col_start, row_start, width, height) = area;

    // Build the whole frame into one buffer and emit a single write, rather than
    // one syscall per cell (width × height per frame on the hot animation path).
    let mut out = String::with_capacity(width as usize * height as usize + height as usize * 8);
    for r in 0..height as usize {
        let _ = write!(
            out,
            "\x1b[{};{}H",
            row_start as usize + r + 1,
            col_start + 1
        );
        for c in 0..width as usize {
            match state.grid.get(r).and_then(|row| row.get(c)) {
                None | Some(None) => out.push(' '),
                Some(Some(cell)) => match age_to_color(cell.age) {
                    None => out.push(' '),
                    Some((red, g, b)) => {
                        let _ = write!(out, "{}", cell.ch.color(owo_colors::Rgb(red, g, b)));
                    }
                },
            }
        }
    }

    eprint!("{out}");
    let _ = io::stderr().flush();
}

// ── Session warp (hyperspace intro / outro) ───────────────────────────────

struct WarpStar {
    angle: f32,
    radius: f32,
    speed: f32,
}

/// 1-based column where a centered line of `width` chars starts. The `+ 1`
/// converts the 0-based margin to a 1-based ANSI column so the line sits truly
/// centered (equal margins) rather than one cell left.
fn center_col(cols: u16, width: usize) -> u16 {
    let margin = (cols as usize).saturating_sub(width) / 2;
    u16::try_from(margin + 1).unwrap_or(1)
}

/// The canonical jackin' logo text — the ` jackin' ` brand pill the host and
/// capsule status bars render (black bold on phosphor-green).
const BRAND_PILL: &str = " jackin' ";

/// Draw the brand pill centered near the bottom of the screen.
fn draw_brand_pill_bottom() {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let row = rows.saturating_sub(2).max(1);
    let col = center_col(cols, BRAND_PILL.chars().count());
    eprint!(
        "\x1b[{row};{col}H{}",
        BRAND_PILL
            .bold()
            .color(rgb((0, 0, 0)))
            .on_color(rgb(PHOSPHOR_GREEN))
    );
}

/// Draw `text` centered on screen with the brand pill below it. This is the
/// held frame shared by the intro phrase animations; it re-reads the live size
/// so callers can re-draw it on every resize.
fn draw_centered_phrase(text: &str, color: (u8, u8, u8)) {
    draw_brand_pill_bottom();
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let (row, col) = (rows / 2, center_col(cols, text.chars().count()));
    eprint!("\x1b[{row};{col}H{}", text.color(rgb(color)));
}

/// Type `text` centered on screen one character at a time, then hold. Returns
/// `true` if the operator skipped with Enter/Esc.
fn type_centered(text: &str, color: (u8, u8, u8), char_ms: u64, hold_ms: u64) -> bool {
    clear_screen();
    draw_brand_pill_bottom();
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let (row, col) = (rows / 2, center_col(cols, text.chars().count()));
    eprint!("\x1b[{row};{col}H");
    for ch in text.chars() {
        eprint!("{}", ch.color(rgb(color)));
        let _ = io::stderr().flush();
        if skippable_sleep(std::time::Duration::from_millis(char_ms)) {
            return true;
        }
    }
    // Hold, re-centering the full phrase + pill on every resize.
    hold_resizable(std::time::Duration::from_millis(hold_ms), || {
        draw_centered_phrase(text, color);
    })
}

/// Glitch-reveal `text` centered on screen (random glyphs settling into the
/// words), then hold. Returns `true` if skipped.
fn glitch_centered(text: &str, color: (u8, u8, u8), hold_ms: u64) -> bool {
    clear_screen();
    draw_brand_pill_bottom();
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let chars: Vec<char> = text.chars().collect();
    let (row, col) = (rows / 2, center_col(cols, chars.len()));
    let mut seed: u64 = 0xCAFE_BABE_1337;
    for _ in 0..5 {
        eprint!("\x1b[{row};{col}H");
        for &ch in &chars {
            let s = xorshift(&mut seed);
            let display = if s.is_multiple_of(3) {
                random_char(&mut seed)
            } else {
                ch
            };
            eprint!("{}", display.color(rgb(color)));
        }
        let _ = io::stderr().flush();
        if skippable_sleep(std::time::Duration::from_millis(70)) {
            break;
        }
    }
    eprint!("\x1b[{row};{col}H{}", text.color(rgb(color)));
    let _ = io::stderr().flush();
    // Hold, re-centering the settled text + pill on every resize.
    hold_resizable(std::time::Duration::from_millis(hold_ms), || {
        draw_centered_phrase(text, color);
    })
}

/// The opening cyberpunk-style call — each phrase shown on its own, centered,
/// in white, before the warp. Each lands, holds, then gives way to the next.
/// Skippable with Enter/Esc.
fn intro_phrases() {
    if type_centered("Stand up, operator...", WHITE, 60, 950) {
        return;
    }
    if type_centered("They're already inside...", WHITE, 55, 950) {
        return;
    }
    if type_centered("Follow the green.", WHITE, 50, 850) {
        return;
    }
    let _ = glitch_centered("Knock, knock, operator.", WHITE, 850);
    clear_screen();
}

/// Discard input events already queued before the intro starts.
///
/// Under `--debug` the operator presses Enter at the plain-CLI "press Enter to
/// continue" gate immediately before this animation. Without draining, that
/// keystroke is still queued when the first `skippable_sleep` polls, so the
/// intro instantly skips itself. Drain once up front so only a keypress made
/// *during* the intro skips it.
fn drain_pending_input() {
    let owns_raw = !crate::tui::host_screen_owned();
    if owns_raw {
        let _ = crossterm::terminal::enable_raw_mode();
    }
    while crossterm::event::poll(std::time::Duration::ZERO).unwrap_or(false) {
        if crossterm::event::read().is_err() {
            break;
        }
    }
    if owns_raw {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

/// Entry ritual — the opening phrases (with the brand pill), then a hyperspace
/// jump *into* the Construct (a starfield accelerating to lightspeed).
pub fn warp_intro() {
    drain_pending_input();
    intro_phrases();
    warp(true);
}

/// Exit ritual — dropping *out* of hyperspace.
///
/// The starfield decelerates from lightspeed to a drift. Played whenever the
/// operator leaves the foreground session, so leaving always feels like slowing
/// down out of the universe.
pub fn warp_out() {
    warp(false);
}

/// The closing screen shown only when the *last* container left (the universe
/// is empty): the brand pill, and how long the operator was in the Construct.
pub fn warp_end_caption(elapsed: Option<std::time::Duration>) {
    clear_screen();
    let _ = hold_resizable(std::time::Duration::from_millis(2400), || {
        let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
        let mid = rows / 2;
        let pill_col = center_col(cols, BRAND_PILL.chars().count());
        eprint!(
            "\x1b[{mid};{pill_col}H{}",
            BRAND_PILL
                .bold()
                .color(rgb((0, 0, 0)))
                .on_color(rgb(PHOSPHOR_GREEN))
        );
        if let Some(d) = elapsed {
            let line = format!("in the Construct for {}", format_universe_duration(d));
            let col = center_col(cols, line.chars().count());
            eprint!(
                "\x1b[{};{col}H{}",
                mid.saturating_add(2),
                line.color(rgb(PHOSPHOR_DIM))
            );
        }
    });
    clear_screen();
}

/// Exit "still running" summary, styled like the intro phrase screens.
///
/// A centered white block — a headline plus one line per still-running
/// workspace/role and a generic folder count — with the brand pill at the
/// bottom. Brief, then clears.
pub fn outro_summary(headline: &str, rows: &[String]) {
    clear_screen();
    let _ = hold_resizable(std::time::Duration::from_millis(2800), || {
        draw_brand_pill_bottom();
        let (cols, term_rows) = crossterm::terminal::size().unwrap_or((80, 24));
        let line_at = |row: u16, text: &str, bold: bool| {
            if row == 0 || row > term_rows {
                return;
            }
            let col = center_col(cols, text.chars().count());
            if bold {
                eprint!("\x1b[{row};{col}H{}", text.bold().color(rgb(WHITE)));
            } else {
                eprint!("\x1b[{row};{col}H{}", text.color(rgb(WHITE)));
            }
        };
        // Center the block (headline + blank + rows) vertically, leaving room
        // above the bottom pill.
        let block_h = rows.len() + 2;
        let top = u16::try_from((term_rows as usize).saturating_sub(block_h + 2) / 2 + 1)
            .unwrap_or(1)
            .max(1);
        line_at(top, headline, true);
        for (i, r) in rows.iter().enumerate() {
            line_at(
                top.saturating_add(2)
                    .saturating_add(u16::try_from(i).unwrap_or(0)),
                r,
                false,
            );
        }
    });
    clear_screen();
}

fn lerp_channel(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (f32::from(b) - f32::from(a))
        .mul_add(t, f32::from(a))
        .round() as u8
}

/// Hyperspace starfield. `accelerating` ramps the warp speed up (entering the
/// universe at increasing velocity); otherwise it ramps back down (dropping to
/// sublight on the way out). Stars stream radially from the center; their
/// trails lengthen and brighten toward white with speed for the lightspeed
/// "wow". Renders raw ANSI like the rest of this module.
#[allow(
    clippy::too_many_lines,
    clippy::suboptimal_flops,
    clippy::type_complexity
)]
fn warp(accelerating: bool) {
    use std::f32::consts::PI;
    use std::fmt::Write as _;

    clear_screen();
    // Hide the cursor and turn off autowrap (DECAWM) for the duration: the warp
    // fills every cell including the bottom-right one, which would scroll the
    // terminal with autowrap on. Off, the field can use the full height.
    eprint!("\x1b[?25l\x1b[?7l");
    let _ = io::stderr().flush();

    // Initial size only seeds the star field; the loop re-reads the live size
    // every frame so the warp tracks terminal resizes.
    let (cols0, rows0) = {
        let (c, r) = crossterm::terminal::size().unwrap_or((80, 24));
        (c as usize, (r as usize).max(1))
    };
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut stars: Vec<WarpStar> = (0..(cols0 * rows0 / 4).clamp(80, 2400))
        .map(|_| {
            let angle = (xorshift(&mut seed) % 36000) as f32 / 36000.0 * 2.0 * PI;
            WarpStar {
                angle,
                radius: (xorshift(&mut seed) % 1000) as f32 / 1000.0
                    * warp_edge_radius(angle, cols0 as f32 / 2.0, rows0 as f32 / 2.0),
                speed: 0.5 + (xorshift(&mut seed) % 100) as f32 / 100.0,
            }
        })
        .collect();

    let frame_ms = 30;
    let frames: usize = 104;
    let mut last_size = (cols0, rows0);
    // Reused across frames; only re-allocated on a terminal resize, otherwise
    // cleared in place so the 104-frame render loop allocates nothing per frame.
    let mut grid: Vec<Vec<Option<(char, (u8, u8, u8))>>> = vec![vec![None; cols0]; rows0];
    let mut out = String::with_capacity(cols0 * rows0 + rows0 * 8);
    for f in 0..frames {
        // Re-read the terminal each frame so a resize mid-warp adapts; clear
        // once on a size change so shrunk-away cells don't linger.
        let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((80, 24));
        let cols = term_cols as usize;
        let rows = (term_rows as usize).max(1);
        if (cols, rows) == last_size {
            for row in &mut grid {
                row.fill(None);
            }
        } else {
            clear_screen();
            last_size = (cols, rows);
            grid = vec![vec![None; cols]; rows];
        }
        let cx = cols as f32 / 2.0;
        let cy = rows as f32 / 2.0;
        // Terminal cells are about twice as tall as wide, so the horizontal
        // projection is stretched ×2 below; `max_r` is just a brightness scale.
        let max_r = (cx / 2.0).hypot(cy).max(1.0);

        let t = f as f32 / frames as f32;
        // Ease the warp factor: accelerate in (slow → blast), decelerate out.
        let warp = if accelerating {
            0.2 + t * t * 5.0
        } else {
            0.2 + (1.0 - t).powi(2) * 5.0
        };
        // Ease the whole field up from black over the first frames so the warp
        // fades in instead of popping on at full brightness.
        let entry_fade = (f as f32 / 8.0).min(1.0);

        for star in &mut stars {
            let prev = star.radius;
            star.radius += star.speed * warp;
            let (dx, dy) = (star.angle.cos() * 2.0, star.angle.sin());
            // Respawn once the head leaves the screen rather than at a fixed
            // radius, so stars travel all the way to the edges and corners and
            // the field fills the whole terminal instead of a central disc.
            let head_x = cx + dx * star.radius;
            let head_y = cy + dy * star.radius;
            if head_x < 0.0 || head_x >= cols as f32 || head_y < 0.0 || head_y >= rows as f32 {
                star.angle = (xorshift(&mut seed) % 36000) as f32 / 36000.0 * 2.0 * PI;
                star.radius = (xorshift(&mut seed) % 60) as f32 / 100.0;
                star.speed = 0.5 + (xorshift(&mut seed) % 100) as f32 / 100.0;
                continue;
            }
            let steps = ((1.0 + warp * 1.4) as usize).max(1);
            for s in 0..=steps {
                let rr = prev + (star.radius - prev) * (s as f32 / steps as f32);
                let x = (cx + dx * rr).round();
                let y = (cy + dy * rr).round();
                if x < 0.0 || y < 0.0 {
                    continue;
                }
                let (xu, yu) = (x as usize, y as usize);
                if xu >= cols || yu >= rows {
                    continue;
                }
                let frac = (rr / max_r).clamp(0.0, 1.0);
                let glyph = if frac > 0.66 {
                    if warp > 2.5 { '─' } else { '*' }
                } else if frac > 0.33 {
                    '+'
                } else {
                    '·'
                };
                // Blue core deepening to bright white streaks toward the edge
                // at speed.
                let bright = (frac * 0.7 + warp / 5.2 * 0.3).clamp(0.0, 1.0);
                let scale = |c: u8| (f32::from(c) * entry_fade) as u8;
                let color = (
                    scale(lerp_channel(60, 235, bright)),
                    scale(lerp_channel(150, 245, bright)),
                    scale(255),
                );
                grid[yu][xu] = Some((glyph, color));
            }
        }

        out.clear();
        for (r, row) in grid.iter().enumerate() {
            let _ = write!(out, "\x1b[{};1H", r + 1);
            for cell in row {
                match cell {
                    None => out.push(' '),
                    Some((ch, (cr, cg, cb))) => {
                        let _ = write!(out, "{}", ch.color(owo_colors::Rgb(*cr, *cg, *cb)));
                    }
                }
            }
        }
        eprint!("{out}");
        let _ = io::stderr().flush();
        if skippable_sleep(std::time::Duration::from_millis(frame_ms)) {
            break;
        }
    }

    clear_screen();
    eprint!("\x1b[H\x1b[?25h\x1b[?7h"); // home + show cursor + restore autowrap
    let _ = io::stderr().flush();
}

/// Radius at which a star at `angle` leaves a `cx`×`cy` half-screen — seeds
/// each star along its own radial out to the edge so the field fills the whole
/// terminal from the first frame instead of a central disc.
fn warp_edge_radius(angle: f32, cx: f32, cy: f32) -> f32 {
    let dx = (angle.cos() * 2.0).abs();
    let dy = angle.sin().abs();
    let rx = if dx > 1e-3 { cx / dx } else { f32::MAX };
    let ry = if dy > 1e-3 { cy / dy } else { f32::MAX };
    rx.min(ry).max(1.0)
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
        assert_eq!(format_universe_duration(Duration::from_mins(134)), "2h 14m");
        assert_eq!(format_universe_duration(Duration::from_secs(0)), "0s");
    }
}
