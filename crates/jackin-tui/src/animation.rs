//! Terminal animation primitives: warp intro/out and duration formatting.
//!
//! Implements the starfield hyperspace entry/exit sequences and closing brand
//! screen. Functions that need raw-mode coordination accept a `host_screen_owned`
//! closure so the caller controls whether this module or the caller owns the
//! terminal's raw mode — avoiding a circular dependency on `jackin-diagnostics`.
//!
//! Not responsible for: full-screen ratatui renders or debug-output lines.
use owo_colors::OwoColorize as _;
use std::io::{self, Write};

use crate::output::clear_screen;
use crate::{
    BRAND_BLOCK, RAIN_BODY, RAIN_DARK, RAIN_DIM, RAIN_FRESH, RAIN_HEAD, RAIN_MID, Rgb, WHITE,
    owo_rgb,
};

fn stderr_fragment(args: std::fmt::Arguments<'_>) {
    let mut stderr = io::stderr().lock();
    drop(write!(stderr, "{args}"));
}

fn flush_stderr() {
    drop(io::stderr().flush());
}

// ── Skippable sleep ─────────────────────────────────────────────────────

/// Sleep for `duration`, but return `true` immediately if Enter or Esc is pressed.
/// `host_screen_owned` reports whether the host already holds raw mode; when
/// `true` this function does not toggle raw mode.
fn skippable_sleep(duration: std::time::Duration, host_screen_owned: bool) -> bool {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};

    // Under the host guard raw mode is already on for the whole flow; toggling
    // it here would hand control back to the cooked terminal mid-animation.
    let owns_raw = !host_screen_owned;
    if owns_raw {
        drop(crossterm::terminal::enable_raw_mode());
    }
    let skipped = if event::poll(duration).unwrap_or(false) {
        matches!(
            event::read(),
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press
                && matches!(key.code, KeyCode::Enter | KeyCode::Esc)
        )
    } else {
        false
    };
    if owns_raw {
        drop(crossterm::terminal::disable_raw_mode());
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
/// terminal resize. Same raw-mode handling as `skippable_sleep`.
fn wait_or_event(duration: std::time::Duration, host_screen_owned: bool) -> WaitOutcome {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    let owns_raw = !host_screen_owned;
    if owns_raw {
        drop(crossterm::terminal::enable_raw_mode());
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
        drop(crossterm::terminal::disable_raw_mode());
    }
    outcome
}

/// Show a static screen for `total`, calling `draw` once up front and again
/// (after a clear) on every terminal resize so the surface always fills and
/// centers to the current size. Returns `true` if the operator skipped.
fn hold_resizable(
    total: std::time::Duration,
    host_screen_owned: bool,
    mut draw: impl FnMut(),
) -> bool {
    draw();
    drop(io::stderr().flush());
    let deadline = std::time::Instant::now() + total;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return false;
        }
        match wait_or_event(remaining, host_screen_owned) {
            WaitOutcome::Skipped => return true,
            WaitOutcome::Resized => {
                clear_screen();
                draw();
                drop(io::stderr().flush());
            }
            WaitOutcome::Elapsed => return false,
        }
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

#[must_use]
pub const fn rain_age_to_color(age: u16) -> Option<Rgb> {
    match age {
        0 => Some(RAIN_HEAD),
        1..=2 => Some(RAIN_FRESH),
        3..=5 => Some(RAIN_BODY),
        6..=10 => Some(RAIN_MID),
        11..=16 => Some(RAIN_DIM),
        17..=24 => Some(RAIN_DARK),
        _ => None,
    }
}

// ── Session warp (hyperspace intro / outro) ───────────────────────────────

struct WarpStar {
    angle: f32,
    radius: f32,
    speed: f32,
}

/// 1-based column where a centered line of `width` chars starts.
fn center_col(cols: u16, width: usize) -> u16 {
    let margin = (cols as usize).saturating_sub(width) / 2;
    u16::try_from(margin + 1).unwrap_or(1)
}

const BRAND_PILL: &str = " jackin❯ ";

fn draw_brand_pill_bottom() {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let row = rows.saturating_sub(2).max(1);
    let col = center_col(cols, BRAND_PILL.chars().count());
    // Green block, black word, white chevron — split so the chevron stays white.
    stderr_fragment(format_args!(
        "\x1b[{row};{col}H{}{}{}",
        " jackin"
            .bold()
            .color(owo_rgb(crate::BLACK))
            .on_color(owo_rgb(BRAND_BLOCK)),
        "❯"
            .bold()
            .color(owo_rgb(WHITE))
            .on_color(owo_rgb(BRAND_BLOCK)),
        " ".on_color(owo_rgb(BRAND_BLOCK)),
    ));
}

fn draw_centered_phrase(text: &str, color: Rgb) {
    draw_brand_pill_bottom();
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let (row, col) = (rows / 2, center_col(cols, text.chars().count()));
    stderr_fragment(format_args!(
        "\x1b[{row};{col}H{}",
        text.color(owo_rgb(color))
    ));
}

fn type_centered(
    text: &str,
    color: Rgb,
    char_ms: u64,
    hold_ms: u64,
    host_screen_owned: bool,
) -> bool {
    clear_screen();
    draw_brand_pill_bottom();
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let (row, col) = (rows / 2, center_col(cols, text.chars().count()));
    stderr_fragment(format_args!("\x1b[{row};{col}H"));
    for ch in text.chars() {
        stderr_fragment(format_args!("{}", ch.color(owo_rgb(color))));
        flush_stderr();
        if skippable_sleep(std::time::Duration::from_millis(char_ms), host_screen_owned) {
            return true;
        }
    }
    hold_resizable(
        std::time::Duration::from_millis(hold_ms),
        host_screen_owned,
        || {
            draw_centered_phrase(text, color);
        },
    )
}

fn glitch_centered(text: &str, color: Rgb, hold_ms: u64, host_screen_owned: bool) -> bool {
    clear_screen();
    draw_brand_pill_bottom();
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let chars: Vec<char> = text.chars().collect();
    let (row, col) = (rows / 2, center_col(cols, chars.len()));
    let mut seed: u64 = 0xCAFE_BABE_1337;
    for _ in 0..5 {
        stderr_fragment(format_args!("\x1b[{row};{col}H"));
        for &ch in &chars {
            let s = xorshift(&mut seed);
            let display = if s.is_multiple_of(3) {
                random_char(&mut seed)
            } else {
                ch
            };
            stderr_fragment(format_args!("{}", display.color(owo_rgb(color))));
        }
        flush_stderr();
        if skippable_sleep(std::time::Duration::from_millis(70), host_screen_owned) {
            break;
        }
    }
    stderr_fragment(format_args!(
        "\x1b[{row};{col}H{}",
        text.color(owo_rgb(color))
    ));
    flush_stderr();
    hold_resizable(
        std::time::Duration::from_millis(hold_ms),
        host_screen_owned,
        || {
            draw_centered_phrase(text, color);
        },
    )
}

fn intro_phrases(host_screen_owned: bool) {
    if type_centered("Stand up, operator...", WHITE, 60, 950, host_screen_owned) {
        return;
    }
    if type_centered("Host stays outside...", WHITE, 55, 950, host_screen_owned) {
        return;
    }
    if type_centered("Follow the green.", WHITE, 50, 850, host_screen_owned) {
        return;
    }
    let _ = glitch_centered("Knock, knock, operator.", WHITE, 850, host_screen_owned);
    clear_screen();
}

fn drain_pending_input(host_screen_owned: bool) {
    let owns_raw = !host_screen_owned;
    if owns_raw {
        drop(crossterm::terminal::enable_raw_mode());
    }
    while crossterm::event::poll(std::time::Duration::ZERO).unwrap_or(false) {
        if crossterm::event::read().is_err() {
            break;
        }
    }
    if owns_raw {
        drop(crossterm::terminal::disable_raw_mode());
    }
}

/// Entry ritual — opening phrases then a hyperspace jump into the Construct.
///
/// `host_screen_owned` should be `jackin_tui::ownership::host_screen_owned()`.
pub fn warp_intro(host_screen_owned: bool) {
    drain_pending_input(host_screen_owned);
    intro_phrases(host_screen_owned);
    warp(true, host_screen_owned);
}

/// Exit ritual — drop out of hyperspace.
///
/// `host_screen_owned` should be `jackin_tui::ownership::host_screen_owned()`.
pub fn warp_out(host_screen_owned: bool) {
    warp(false, host_screen_owned);
}

/// Closing screen shown when the last container leaves.
///
/// `host_screen_owned` should be `jackin_tui::ownership::host_screen_owned()`.
pub fn warp_end_caption(elapsed: Option<std::time::Duration>, host_screen_owned: bool) {
    if let Some(d) = elapsed {
        let line = format!(
            "You were in the Construct for {}",
            format_universe_duration(d)
        );
        // Same glitch-in reveal as the intro phrases (e.g. "Knock, knock, operator.").
        let _ = glitch_centered(&line, WHITE, 2400, host_screen_owned);
    }
    clear_screen();
}

fn lerp_channel(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    #[expect(
        clippy::cast_sign_loss,
        reason = "t clamped to 0.0..=1.0; lerp stays in u8 range"
    )]
    {
        (f32::from(b) - f32::from(a))
            .mul_add(t, f32::from(a))
            .round() as u8
    }
}

#[allow(
    clippy::too_many_lines,
    clippy::suboptimal_flops,
    clippy::type_complexity,
    reason = "documented residual allow; prefer expect when site is lint-true")]
#[allow(
    clippy::excessive_nesting,
    reason = "Star-warp rendering loop: per-frame, per-star, per-step nested \
              control flow over the cell grid + radials. Extracting per-star \
              helpers would require re-borrowing the grid + stars iterators \
              across fn boundaries and obscure the per-step composition."
)]
fn warp(accelerating: bool, host_screen_owned: bool) {
    use std::f32::consts::PI;
    use std::fmt::Write as _;

    clear_screen();
    stderr_fragment(format_args!("\x1b[?25l\x1b[?7l"));
    flush_stderr();

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
    let mut grid: Vec<Vec<Option<(char, (u8, u8, u8))>>> = vec![vec![None; cols0]; rows0];
    let mut out = String::with_capacity(cols0 * rows0 + rows0 * 8);
    for f in 0..frames {
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
        let max_r = (cx / 2.0).hypot(cy).max(1.0);

        let t = f as f32 / frames as f32;
        let warp_factor = if accelerating {
            0.2 + t * t * 5.0
        } else {
            0.2 + (1.0 - t).powi(2) * 5.0
        };
        let entry_fade = (f as f32 / 8.0).min(1.0);

        for star in &mut stars {
            let prev = star.radius;
            star.radius += star.speed * warp_factor;
            let (dx, dy) = (star.angle.cos() * 2.0, star.angle.sin());
            let head_x = cx + dx * star.radius;
            let head_y = cy + dy * star.radius;
            if head_x < 0.0 || head_x >= cols as f32 || head_y < 0.0 || head_y >= rows as f32 {
                star.angle = (xorshift(&mut seed) % 36000) as f32 / 36000.0 * 2.0 * PI;
                star.radius = (xorshift(&mut seed) % 60) as f32 / 100.0;
                star.speed = 0.5 + (xorshift(&mut seed) % 100) as f32 / 100.0;
                continue;
            }
            #[expect(
                clippy::cast_sign_loss,
                reason = "warp_factor is non-negative; steps is at least 1"
            )]
            let steps = ((1.0 + warp_factor * 1.4) as usize).max(1);
            for s in 0..=steps {
                let rr = prev + (star.radius - prev) * (s as f32 / steps as f32);
                let x = (cx + dx * rr).round();
                let y = (cy + dy * rr).round();
                if x < 0.0 || y < 0.0 {
                    continue;
                }
                #[expect(clippy::cast_sign_loss, reason = "x/y rejected when negative above")]
                let (xu, yu) = (x as usize, y as usize);
                if xu >= cols || yu >= rows {
                    continue;
                }
                let frac = (rr / max_r).clamp(0.0, 1.0);
                let glyph = if frac > 0.66 {
                    if warp_factor > 2.5 { '─' } else { '*' }
                } else if frac > 0.33 {
                    '+'
                } else {
                    '·'
                };
                let bright = (frac * 0.7 + warp_factor / 5.2 * 0.3).clamp(0.0, 1.0);
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "entry_fade is a non-negative animation alpha"
                )]
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
            let _unused = write!(out, "\x1b[{};1H", r + 1);
            for cell in row {
                match cell {
                    None => out.push(' '),
                    Some((ch, (cr, cg, cb))) => {
                        let _unused = write!(out, "{}", ch.color(owo_colors::Rgb(*cr, *cg, *cb)));
                    }
                }
            }
        }
        stderr_fragment(format_args!("{out}"));
        flush_stderr();
        if skippable_sleep(
            std::time::Duration::from_millis(frame_ms),
            host_screen_owned,
        ) {
            break;
        }
    }

    clear_screen();
    stderr_fragment(format_args!("\x1b[H\x1b[?25h\x1b[?7h"));
    flush_stderr();
}

fn warp_edge_radius(angle: f32, cx: f32, cy: f32) -> f32 {
    let dx = (angle.cos() * 2.0).abs();
    let dy = angle.sin().abs();
    let rx = if dx > 1e-3 { cx / dx } else { f32::MAX };
    let ry = if dy > 1e-3 { cy / dy } else { f32::MAX };
    rx.min(ry).max(1.0)
}

/// Format a session duration compactly: `2h 14m`, `7m 30s`, or `45s`.
#[must_use]
/// Human-readable session length: the two largest non-zero units, worded and
/// pluralized — e.g. `1 day 3 hours`, `28 minutes 17 seconds`, `45 seconds`.
pub fn format_universe_duration(d: std::time::Duration) -> String {
    fn unit(n: u64, name: &str) -> String {
        format!("{n} {name}{}", if n == 1 { "" } else { "s" })
    }

    let secs = d.as_secs();
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if days > 0 {
        format!("{} {}", unit(days, "day"), unit(hours, "hour"))
    } else if hours > 0 {
        format!("{} {}", unit(hours, "hour"), unit(minutes, "minute"))
    } else if minutes > 0 {
        format!("{} {}", unit(minutes, "minute"), unit(seconds, "second"))
    } else {
        unit(seconds, "second")
    }
}
