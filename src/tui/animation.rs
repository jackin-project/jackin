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

// ── Session warp (hyperspace intro / outro) ───────────────────────────────

struct WarpStar {
    angle: f32,
    radius: f32,
    speed: f32,
}

/// Entry ritual — a hyperspace jump *into* the Construct: a starfield that
/// streaks outward from the center and accelerates to lightspeed, then a calm
/// caption with the phrase of the day.
pub fn warp_intro() {
    warp(true);
    warp_caption(super::quotes::pick(super::quotes::START_QUOTES), None);
}

/// Exit ritual — dropping *out* of hyperspace: the starfield decelerates from
/// lightspeed to a drift, then a caption with how long the operator was in the
/// Construct.
pub fn warp_outro(elapsed: Option<std::time::Duration>) {
    warp(false);
    let footer = elapsed.map(|d| format!("in the Construct for {}", format_universe_duration(d)));
    warp_caption(super::quotes::pick(super::quotes::END_QUOTES), footer.as_deref());
}

fn lerp_channel(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (f32::from(b) - f32::from(a)).mul_add(t, f32::from(a)).round() as u8
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
    eprint!("\x1b[?25l"); // hide cursor
    let _ = io::stderr().flush();

    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let cols = term_cols as usize;
    let rows = (term_rows as usize).saturating_sub(1).max(1);
    let cx = cols as f32 / 2.0;
    let cy = rows as f32 / 2.0;
    // Terminal cells are about twice as tall as wide, so the horizontal
    // projection is stretched ×2 below; size the field to the half-width.
    let max_r = (cx / 2.0).hypot(cy).max(1.0);

    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut stars: Vec<WarpStar> = (0..(cols * rows / 5).clamp(60, 1500))
        .map(|_| WarpStar {
            angle: (xorshift(&mut seed) % 36000) as f32 / 36000.0 * 2.0 * PI,
            radius: (xorshift(&mut seed) % 1000) as f32 / 1000.0 * max_r,
            speed: 0.5 + (xorshift(&mut seed) % 100) as f32 / 100.0,
        })
        .collect();

    let frame_ms = 28;
    let frames: usize = 56;
    for f in 0..frames {
        let t = f as f32 / frames as f32;
        // Ease the warp factor: accelerate in (slow → blast), decelerate out.
        let warp = if accelerating {
            0.2 + t * t * 5.0
        } else {
            0.2 + (1.0 - t).powi(2) * 5.0
        };

        let mut grid: Vec<Vec<Option<(char, (u8, u8, u8))>>> = vec![vec![None; cols]; rows];
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
            let steps = (1.0 + warp * 1.4) as usize;
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
                let color = (
                    lerp_channel(60, 235, bright),
                    lerp_channel(150, 245, bright),
                    255,
                );
                grid[yu][xu] = Some((glyph, color));
            }
        }

        let mut out = String::with_capacity(cols * rows + rows * 8);
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

    for r in 0..rows {
        eprint!("\x1b[{};1H\x1b[2K", r + 1);
    }
    eprint!("\x1b[H\x1b[?25h"); // home + show cursor
    let _ = io::stderr().flush();
}

/// The jackin' logo, drawn on the calm caption screen after the warp.
const LOGO: &[&str] = &[
    "\u{2502} \u{2502}\u{2577}\u{2502} \u{2502}\u{2577}\u{2502} \u{2577}  \u{2502}\u{2577}\u{2502} \u{2502}\u{2577}\u{2502} \u{2502}\u{2577}\u{2502}",
    "\u{2502} \u{2575}\u{2502} \u{2502}\u{2575}\u{2502} \u{2575} \u{2577} \u{2575}\u{2502} \u{2502}\u{2575}\u{2502} \u{2502}\u{2575}\u{2502}",
    "\u{2575}  \u{2575} \u{2575} \u{2575}  \u{2502}  \u{2575} \u{2575} \u{2575} \u{2575} \u{2575}",
    "           \u{2575}",
    "      j a c k i n",
    "   operator terminal",
];

/// Calm caption shown after the warp settles: the jackin' logo, then the
/// phrase of the day, then an optional footer line. Centered as one block.
/// Brief, then clears.
fn warp_caption(quote: Option<&super::quotes::Quote>, footer: Option<&str>) {
    clear_screen();
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

    let logo_h = u16::try_from(LOGO.len()).unwrap_or(0);
    // Vertically center the whole block: logo + blank + quote(2) + footer(2).
    let mut block = logo_h + 1;
    if quote.is_some() {
        block += 2;
    }
    if footer.is_some() {
        block += 2;
    }
    let top = term_rows.saturating_sub(block) / 2 + 1;

    // Logo lines share one left column so the art stays aligned (per-line
    // centering would skew the rows that carry leading spaces).
    let logo_w = LOGO.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let logo_col = u16::try_from(cols.saturating_sub(logo_w) / 2).unwrap_or(0).max(1);
    for (i, line) in LOGO.iter().enumerate() {
        let row = top + u16::try_from(i).unwrap_or(0);
        if row <= term_rows {
            eprint!("\x1b[{row};{logo_col}H{}", line.color(rgb(PHOSPHOR_GREEN)));
        }
    }

    let mut row = top + logo_h + 1;
    if let Some(q) = quote {
        center(row, &format!("\u{201C}{}\u{201D}", q.text), WHITE);
        center(row + 1, &format!("\u{2014} {}", q.author), PHOSPHOR_DIM);
        row += 3;
    }
    if let Some(f) = footer {
        center(row, f, PHOSPHOR_DIM);
    }
    let _ = io::stderr().flush();
    let _ = skippable_sleep(std::time::Duration::from_millis(1700));
    clear_screen();
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
