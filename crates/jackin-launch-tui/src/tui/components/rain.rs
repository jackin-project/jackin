//! Digital-rain simulation for the launch cockpit.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use jackin_tui::{RAIN_BODY, RAIN_DARK, RAIN_DIM, RAIN_FRESH, RAIN_HEAD, RAIN_MID, Rgb};

#[derive(Debug, Clone)]
pub struct RainCell {
    pub ch: char,
    pub age: u16,
    /// How many age units to add per frame (1 = long trail, 3 = short trail).
    pub fade: u16,
}

#[derive(Debug, Clone)]
pub struct RainColumn {
    pub head: i32,
    pub speed: u32,
    /// Fade rate for cells deposited by this column (1 = long, 3 = short).
    pub fade: u16,
    pub active: bool,
    pub cooldown: u32,
}

#[derive(Debug, Clone)]
pub struct RainState {
    pub grid: Vec<Vec<Option<RainCell>>>,
    pub columns: Vec<RainColumn>,
    pub cols: usize,
    pub rows: usize,
    pub seed: u64,
    pub frame: u64,
}

impl RainState {
    #[must_use]
    pub fn new(cols: usize, rows: usize) -> Self {
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

#[must_use]
pub const fn age_to_color(age: u16) -> Option<Rgb> {
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

/// Advance the rain state by one tick: age existing cells and move column
/// heads forward. This is the simulation step; callers render the result
/// after ticking.
pub fn tick_rain(state: &mut RainState) {
    let RainState {
        grid,
        columns,
        rows,
        seed,
        frame,
        ..
    } = state;

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

/// Paint the shared rain engine's grid into `area`. The grid is sized to the
/// whole terminal, so `area` is a window onto a continuous rainfall; each cell
/// maps to its glyph and the engine's green age fade — the same palette as the
/// intro/outro rain.
pub fn render_rain(frame: &mut Frame<'_>, area: Rect, rain: Option<&RainState>) {
    let Some(rain) = rain else {
        return;
    };
    if area.width == 0 || area.height == 0 {
        return;
    }
    // Fade the whole field up from black over the first ~30 ticks so the rain
    // eases in smoothly instead of popping on at full brightness.
    let fade_in = (rain.frame as f32 / 30.0).min(1.0);
    // Fade the rain to black over the bottom rows so it dissolves into a gap
    // above the progress bar instead of colliding with it: the bottommost row
    // is fully extinguished and brightness ramps back to full a few rows up.
    let fade_rows = (area.height / 3).clamp(3, 7);
    // Write each cell straight into the frame buffer rather than building a
    // `Vec<Line<Span>>`: at 30fps a full field is width × height spans, each its
    // own `String`, every frame. RAIN_CHARS is ASCII (width-1), so one cell maps
    // to one buffer cell. An empty cell only sets its symbol so it keeps the
    // background already painted behind the rain.
    let buf = frame.buffer_mut();
    for y in 0..area.height {
        let grid_y = usize::from(area.y + y);
        let rows_from_bottom = area.height - 1 - y;
        let fade = if rows_from_bottom >= fade_rows {
            1.0
        } else {
            f32::from(rows_from_bottom) / f32::from(fade_rows)
        };
        let dim = |c: u8| (f32::from(c) * fade * fade_in) as u8;
        for x in 0..area.width {
            let grid_x = usize::from(area.x + x);
            let lit = rain
                .grid
                .get(grid_y)
                .and_then(|row| row.get(grid_x))
                .and_then(|cell| cell.as_ref())
                .and_then(|cell| age_to_color(cell.age).map(|rgb| (cell.ch, rgb)));
            let cell = &mut buf[(area.x + x, area.y + y)];
            match lit {
                Some((ch, Rgb { r, g, b })) => {
                    cell.set_char(ch);
                    cell.set_style(Style::default().fg(Color::Rgb(dim(r), dim(g), dim(b))));
                }
                None => {
                    cell.set_char(' ');
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
