use jackin_brand::Rgb;
use std::io::Write as _;

pub const BG_DARK: &str = "\x1b[49m";
pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";

pub const INVERSE: &str = "\x1b[7m";

pub const BRAND_BANNER: &str = "\n  \x1b[1m\x1b[48;2;0;255;65m\x1b[38;2;0;0;0m jackin\x1b[38;2;255;255;255m❯\x1b[38;2;0;0;0m \x1b[0m\n";

#[must_use]
pub fn version_splash(version: &str) -> String {
    let pill = "\x1b[1m\x1b[48;2;0;255;65m\x1b[38;2;0;0;0m jackin\x1b[38;2;255;255;255m❯\x1b[38;2;0;0;0m \x1b[0m";
    format!(
        "\n  {pill}  \x1b[38;2;0;255;65m{version}\x1b[0m\n  \x1b[38;2;94;106;100mby tailrocks\x1b[0m\n"
    )
}

#[must_use]
pub fn help_banner(width: u16) -> String {
    const H: usize = 13;

    const FADE_ROWS: usize = 4;
    const POOL: &[u8] =
        b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz@#$%&*<>{}[]|/\\~";
    let w = usize::from(width.saturating_sub(4)).clamp(48, 76);

    let pill = "\x1b[1m\x1b[48;2;0;255;65m\x1b[38;2;0;0;0m jackin\x1b[38;2;255;255;255m❯\x1b[38;2;0;0;0m \x1b[0m";
    const PILL_W: usize = 9;
    let byline = "by tailrocks";
    let byl_w = byline.len();
    let lock_row = H / 2 - 1;
    let byl_row = H / 2 + 1;
    let lc = w.saturating_sub(PILL_W) / 2;
    let bc = w.saturating_sub(byl_w) / 2;

    let box_top = lock_row - 1;
    let box_bot = byl_row + 1;
    let box_lo = lc.min(bc).saturating_sub(3);
    let box_hi = (lc + PILL_W).max(bc + byl_w) + 3;

    let xorshift = |mut s: u64| -> u64 {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };

    let mut out = String::from("\n");
    for r in 0..H {
        out.push_str("  ");
        let mut c = 0;
        while c < w {
            if r == lock_row && c == lc {
                out.push_str(pill);
                c += PILL_W;
                continue;
            }
            if r == byl_row && c == bc {
                out.push_str(&format!("\x1b[38;2;94;106;100m{byline}\x1b[0m"));
                c += byl_w;
                continue;
            }
            if r >= box_top && r <= box_bot && c >= box_lo && c < box_hi {
                out.push(' ');
                c += 1;
                continue;
            }

            let col_seed = xorshift((c as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x5EED);
            let lit = if col_seed % 100 < 55 {
                let head = (col_seed >> 8) % (H as u64 + 6);
                let fade = 1 + (col_seed >> 24) % 3;
                if (r as u64) <= head {
                    rain_age_to_color(((head - r as u64) * fade) as u16)
                } else {
                    None
                }
            } else {
                None
            };
            match lit {
                Some(rgb) => {
                    let vdist = box_top.saturating_sub(r).max(r.saturating_sub(box_bot));
                    let num = vdist.min(FADE_ROWS) as u16;
                    let dim = |x: u8| ((u16::from(x) * num) / FADE_ROWS as u16) as u8;
                    let (rr, gg, bb) = (dim(rgb.r), dim(rgb.g), dim(rgb.b));
                    if rr == 0 && gg == 0 && bb == 0 {
                        out.push(' ');
                    } else {
                        let g = xorshift(
                            (r as u64).wrapping_mul(0xD1B5_4A32_D192_ED03)
                                ^ (c as u64).wrapping_mul(0x2545_F491_4F6C_DD1D)
                                ^ 0xCAFE_F00D,
                        );
                        let ch = POOL[(g as usize) % POOL.len()] as char;
                        out.push_str(&format!("\x1b[38;2;{rr};{gg};{bb}m{ch}"));
                        out.push_str(RESET);
                    }
                }
                None => out.push(' '),
            }
            c += 1;
        }
        while out.ends_with(' ') {
            out.pop();
        }
        out.push('\n');
    }
    out
}

pub const HELP_BANNER_MIN_COLS: u16 = 60;

pub const fn rgb_fg(rgb: Rgb) -> &'static str {
    match (rgb.r, rgb.g, rgb.b) {
        (0, 255, 65) => "\x1b[38;2;0;255;65m",
        (0, 140, 30) => "\x1b[38;2;0;140;30m",
        (0, 80, 18) => "\x1b[38;2;0;80;18m",
        (0, 80, 180) => "\x1b[38;2;0;80;180m",
        (80, 80, 80) => "\x1b[38;2;80;80;80m",
        (255, 255, 255) => "\x1b[38;2;255;255;255m",
        (0, 0, 0) => "\x1b[38;2;0;0;0m",
        (255, 170, 0) => "\x1b[38;2;255;170;0m",
        (180, 255, 180) => "\x1b[38;2;180;255;180m",
        _ => "",
    }
}

pub const fn rgb_bg(rgb: Rgb) -> &'static str {
    match (rgb.r, rgb.g, rgb.b) {
        (0, 255, 65) => "\x1b[48;2;0;255;65m",
        (42, 42, 42) => "\x1b[48;2;42;42;42m",
        (255, 255, 255) => "\x1b[48;2;255;255;255m",
        (0, 0, 0) => "\x1b[48;2;0;0;0m",
        _ => "",
    }
}

#[must_use]
pub fn rgb_fg_dyn(rgb: Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m", rgb.r, rgb.g, rgb.b)
}

#[must_use]
pub fn rgb_bg_dyn(rgb: Rgb) -> String {
    format!("\x1b[48;2;{};{};{}m", rgb.r, rgb.g, rgb.b)
}

pub fn emit_osc8_open(buf: &mut Vec<u8>, href: &str) {
    buf.extend_from_slice(b"\x1b]8;;");
    buf.extend_from_slice(href.as_bytes());
    buf.extend_from_slice(b"\x1b\\");
}

pub fn emit_osc8_close(buf: &mut Vec<u8>) {
    buf.extend_from_slice(b"\x1b]8;;\x1b\\");
}

pub fn move_to(buf: &mut Vec<u8>, row: u16, col: u16) {
    let _unused = write!(buf, "\x1b[{};{}H", row + 1, col + 1);
}

pub fn fg(buf: &mut Vec<u8>, rgb: Rgb) {
    let _unused = write!(buf, "\x1b[38;2;{};{};{}m", rgb.r, rgb.g, rgb.b);
}

pub fn bg(buf: &mut Vec<u8>, rgb: Rgb) {
    let _unused = write!(buf, "\x1b[48;2;{};{};{}m", rgb.r, rgb.g, rgb.b);
}

const fn rain_age_to_color(age: u16) -> Option<Rgb> {
    match age {
        0 => Some(jackin_brand::RAIN_HEAD),
        1..=2 => Some(jackin_brand::RAIN_FRESH),
        3..=5 => Some(jackin_brand::RAIN_BODY),
        6..=10 => Some(jackin_brand::RAIN_MID),
        11..=16 => Some(jackin_brand::RAIN_DIM),
        17..=24 => Some(jackin_brand::RAIN_DARK),
        _ => None,
    }
}
