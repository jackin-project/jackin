//! Shared TUI tokens, models, and components used by jackin❯'s
//! terminal surfaces.
//!
//! Backend-neutral types such as RGB tokens, tab-cell layout, hint
//! spans, text-field state, and scroll metrics stay at the crate
//! root or in small helper modules. Ratatui components live under
//! [`components`], with color adapters in [`theme`]. Surface crates
//! own domain state and compose these pieces instead of re-declaring
//! palette values or reimplementing visual primitives.
//!
//! # Architecture Invariant
//!
//! `jackin-tui` is a **presentation-layer** design-system crate.
//! Allowed upstream dependencies: `jackin-core` (domain vocabulary only).
//! Must **not** depend on any application, infrastructure, or entry crate
//! (`jackin-runtime`, `jackin-env`, `jackin-docker`, `jackin-launch*`,
//! `jackin-console`, `jackin-capsule`, or `jackin`).
//!
//! The shared Elm runtime contract lives in [`runtime`]: one
//! [`runtime::UpdateResult`] return per `update` call,
//! [`runtime::Component`] for event→message translation, and
//! [`runtime::View`] for model→frame rendering. Surface crates
//! implement these traits; `jackin-tui` only defines them.

pub mod animation;
pub mod ansi_text;
pub mod components;
pub mod geometry;
pub mod host_colors;
pub mod keymap;
pub mod output;
pub mod prune_output;
pub mod runtime;
pub mod scroll;
pub mod terminal_modes;
pub mod theme;
pub mod url_text;

pub use components::text_input::TextField;
pub use geometry::{
    FixedPrefixSegment, HintSpan, TAB_GAP, TabCell, agent_display_name, centered_rect,
    display_cols, display_cols_slice, fixed_prefix_scroll_segments, hint_row_cols,
    is_terminal_control_char, lay_out_tabs, leading_space_cols, padded_line_display_cols,
    sanitize_terminal_title, tab_at_column, take_display_cols,
};
pub use jackin_core::shorten_home;

/// Outcome of a modal or component event-handling cycle.
///
/// Surface-specific state machines use this to decide whether to keep a
/// component open, commit its value, or cancel the interaction. Keeping the
/// type in `jackin-tui` lets host, launch, and capsule components share the
/// same update contract without depending on one surface's widget module.
#[derive(Debug, Clone)]
pub enum ModalOutcome<T> {
    /// User is still interacting with the component.
    Continue,
    /// User committed with this value.
    Commit(T),
    /// User cancelled the interaction.
    Cancel,
}

/// Three-byte RGB triple. Constructors below are the canonical
/// phosphor palette used everywhere a jackin TUI surface needs to
/// pick a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Adapt an [`Rgb`] token to the `owo_colors` raw-ANSI colour type used by the
/// stderr output, spinner, and animation helpers across every surface crate.
#[must_use]
pub fn owo_rgb(rgb: Rgb) -> owo_colors::Rgb {
    owo_colors::Rgb(rgb.r, rgb.g, rgb.b)
}

/// `--jk-brand` — the bright phosphor green used for selection
/// highlights, the row-0 brand pill, and live indicators.
pub const PHOSPHOR_GREEN: Rgb = Rgb::new(0, 255, 65);

/// Mid-green used for inactive tab labels, dim labels, and "Dyn"
/// footer text in the console.
pub const PHOSPHOR_DIM: Rgb = Rgb::new(0, 140, 30);

/// Dark green used for panel borders and dot separators.
pub const PHOSPHOR_DARK: Rgb = Rgb::new(0, 80, 18);

/// Pure black base colour.
pub const BLACK: Rgb = Rgb::new(0, 0, 0);

// Dialog backdrop and surface deliberately have no RGB token: both paint the
// terminal's DEFAULT background, not a fixed colour, so overlays match the
// operator's terminal theme instead of forcing pure black that stands out
// against a themed (non-black) default. The Ratatui side is
// `theme::DIALOG_BACKDROP` / `theme::DIALOG_SURFACE` (= `Color::Reset`); the
// raw-ANSI side is `ansi::BG_DARK` (= `\x1b[49m`, default-background SGR).

/// Focused scroll/thumb accent for modal scroll regions.
pub const DIALOG_SCROLL_THUMB: Rgb = PHOSPHOR_GREEN;

/// Scroll track and unfocused dialog border colour.
pub const DIALOG_SCROLL_TRACK: Rgb = PHOSPHOR_DARK;

/// Bright white rain head used by the launch digital-rain animation.
pub const RAIN_HEAD: Rgb = WHITE;

/// Fresh rain trail immediately behind the head.
pub const RAIN_FRESH: Rgb = Rgb::new(180, 255, 180);

/// Brand-green rain trail at normal brightness.
pub const RAIN_BODY: Rgb = PHOSPHOR_GREEN;

/// Mid-bright rain trail between the brand and dim greens.
pub const RAIN_MID: Rgb = Rgb::new(0, 200, 50);

/// Dim rain trail.
pub const RAIN_DIM: Rgb = PHOSPHOR_DIM;

/// Dark trailing rain tail.
pub const RAIN_DARK: Rgb = PHOSPHOR_DARK;

/// White used for titles, hotkey glyphs, and the active-tab underline.
pub const WHITE: Rgb = Rgb::new(255, 255, 255);

/// Logo block green: the canonical phosphor green (`#00FF41`), the same green
/// the digital rain uses. The brand pill sits on this block with a black word
/// and a white chevron — the logo uses the real jackin green, not the muted
/// `#5CF07A` chevron-accent.
pub const BRAND_BLOCK: Rgb = PHOSPHOR_GREEN;

/// Almost-invisible dim background for the input band inside a
/// text-input dialog. Picked so the input region is visible even when
/// empty without competing with the dialog's `PHOSPHOR_DARK` border.
/// Used by the host TUI's `text_input` widget and the
/// `jackin-capsule` rename dialog so both surfaces share the same
/// "this is where you type" cue.
pub const INPUT_BG_DIM: Rgb = Rgb::new(20, 24, 22);

/// Tab-cell backgrounds shared by the in-container multiplexer status bar
/// (`jackin-capsule`) and the host console tab strips (workspace editor,
/// settings) so the two surfaces render identical tab chrome. Inactive
/// tabs sit on a subtle dark grey; the active tab lifts to a graphite that
/// stays distinct from the brand-green pill; hover lifts each one cell
/// further.
pub const TAB_BG_INACTIVE: Rgb = Rgb::new(30, 30, 30);
pub const TAB_BG_INACTIVE_HOVER: Rgb = Rgb::new(48, 48, 48);
pub const TAB_BG_ACTIVE: Rgb = Rgb::new(42, 42, 42);
pub const TAB_BG_ACTIVE_HOVER: Rgb = Rgb::new(58, 58, 58);

/// Link/clickable foreground used on the white bottom status bar (the
/// container/instance-id chip) by both the in-container multiplexer and the
/// host loading screen, so a clickable id reads the same on both surfaces.
/// Reserved for clickable text on a *light* (white) background, where the
/// dark-surface `LINK_FG` cyan would have too little contrast.
pub const LINK_BLUE: Rgb = Rgb::new(0, 80, 180);

/// Copyable / clickable value foreground on a *dark* dialog surface. Used by
/// every "Debug info" row whose value can be clicked to copy (paths, IDs) so
/// the affordance reads identically across the console, launch cockpit, and
/// capsule. Cyan, not blue: distinct from the brand-green focus colour and
/// readable on the black dialog backdrop. Always paired with an underline so
/// the value reads as a link per the W3C native-link convention.
pub const LINK_FG: Rgb = Rgb::new(0, 200, 200);

/// Hover foreground for a copyable value — a brighter cyan than [`LINK_FG`].
/// The colour shift on pointer hover is the visible feedback that the value is
/// interactive (W3C native-link hover behaviour).
pub const LINK_FG_HOVER: Rgb = Rgb::new(130, 240, 240);

/// Burnt orange marking debug-mode chrome — the run-id chip on the status
/// bar renders in this so the operator can tell at a glance they are inside
/// a `--debug` run. Readable on the white status-bar band.
pub const DEBUG_AMBER: Rgb = Rgb::new(204, 92, 0);

/// Amber — used for the Stuck tab glyph and token rate bar below 20% threshold.
/// `#ffaa00`
pub const AMBER: Rgb = Rgb::new(255, 170, 0);

/// Neutral gray for unfocused chrome borders — the in-container multiplexer's
/// inactive pane border and the host's full-screen non-interactive frames
/// (the launch cockpit box, the exit summary box) so chrome reads identically
/// across surfaces and stays out of the way of focused, brand-green content.
pub const BORDER_GRAY: Rgb = Rgb::new(80, 80, 80);

/// Lighter neutral gray used for unfocused scroll thumbs on pane borders.
pub const BORDER_GRAY_LIGHT: Rgb = Rgb::new(160, 160, 160);

/// Error/danger accent — failed launch stages, error-popup borders, invalid
/// input fields, and danger labels. Shared across every TUI surface so the
/// "something went wrong" colour never drifts between the console widgets and
/// the launch cockpit.
pub const DANGER_RED: Rgb = Rgb::new(255, 94, 122);

/// Status-tab blocked glyph: saturated red reserved for "waiting for operator".
pub const STATUS_BLOCKED_RED: Rgb = Rgb::new(255, 60, 60);

/// Capsule menu button background, idle state.
pub const CAPSULE_MENU_IDLE_BG: Rgb = Rgb::new(18, 70, 130);

/// Capsule menu button background while pointer-hovered.
pub const CAPSULE_MENU_IDLE_HOVER_BG: Rgb = Rgb::new(32, 92, 158);

/// Capsule menu button background while the prefix key is awaiting a command.
pub const CAPSULE_MENU_AWAITING_BG: Rgb = Rgb::new(96, 180, 255);

/// Capsule menu button background for hovered awaiting state.
pub const CAPSULE_MENU_AWAITING_HOVER_BG: Rgb = Rgb::new(132, 202, 255);

/// Live / active state indicator (cyan). Shared between the editor's
/// running-instance status badge and any other "this is live" cue.
pub const CYAN: Rgb = Rgb::new(0, 180, 180);

/// Dimmed cyan for secondary live-state text.
pub const CYAN_DIM: Rgb = Rgb::new(0, 120, 120);

/// Light-green accent used for permitted-action markers and similar
/// affirmative highlights.
pub const ACTION_ACCENT: Rgb = Rgb::new(180, 255, 180);

/// Amber-yellow accent used for disclosure indicators (expandable
/// sections, trust prompts, and similar expand/collapse cues).
pub const DISCLOSURE_ACCENT: Rgb = Rgb::new(255, 208, 102);

/// Warm yellow used for warning notes inside confirmation dialogs.
pub const WARNING_YELLOW: Rgb = Rgb::new(255, 216, 94);

/// Dark charcoal canvas fill for the lookbook preview card. Sits between
/// pure black and the phosphor-dark panel borders so components have a
/// distinct bounded backdrop without the green tint of `PHOSPHOR_DARK`.
pub const PREVIEW_CARD: Rgb = Rgb::new(28, 28, 28);

/// Shared ANSI helpers.
pub mod ansi {
    use super::Rgb;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use std::io::Write as _;

    /// Dialog surface / input-band background SGR. Emits the terminal's DEFAULT
    /// background (`\x1b[49m`), not a fixed colour, so raw-ANSI overlays match
    /// the operator's terminal theme instead of forcing pure black.
    pub const BG_DARK: &str = "\x1b[49m";
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";

    /// OSC 22 cursor-shape escapes. `POINTER_HAND` switches the terminal
    /// pointer to the hand/`pointer` shape over a clickable element;
    /// `POINTER_DEFAULT` restores it. Shared by every TUI surface so the
    /// "this is clickable" cue is identical (terminals without OSC 22 ignore
    /// the sequence harmlessly).
    pub const POINTER_HAND: &str = "\x1b]22;pointer\x1b\\";
    pub const POINTER_DEFAULT: &str = "\x1b]22;default\x1b\\";
    pub const INVERSE: &str = "\x1b[7m";

    /// Help/banner form of the brand pill, shared with the host and
    /// capsule status bars so every surface shows the same logo.
    pub const BRAND_BANNER: &str = "\n  \x1b[1m\x1b[48;2;0;255;65m\x1b[38;2;0;0;0m jackin\x1b[38;2;255;255;255m❯\x1b[38;2;0;0;0m \x1b[0m\n";

    /// Multi-line `jackin --version` splash for an interactive terminal: the
    /// green-block `jackin❯` pill, the version string, and the `by tailrocks`
    /// byline. Stays under six lines and is brand-aligned — the mark is the
    /// terminal pill, never large ASCII or illustration art. Piped output gets clap's
    /// plain `jackin <version>` instead.
    #[must_use]
    pub fn version_splash(version: &str) -> String {
        let pill = "\x1b[1m\x1b[48;2;0;255;65m\x1b[38;2;0;0;0m jackin\x1b[38;2;255;255;255m❯\x1b[38;2;0;0;0m \x1b[0m";
        format!(
            "\n  {pill}  \x1b[38;2;0;255;65m{version}\x1b[0m\n  \x1b[38;2;94;106;100mby tailrocks\x1b[0m\n"
        )
    }

    /// Frozen digital-rain banner with the `jackin❯` lockup at its centre, for
    /// the root `jackin --help` on a wide interactive terminal. A single frame
    /// of the launch cockpit's rain — per-column drops with a white head and a
    /// trail fading up through phosphor to dark (shared `RAIN_*` palette and age
    /// ramp) — that dims toward the centre so the rain dissolves into the logo,
    /// the same way the launch rain fades into the loading bar. Sized to the
    /// terminal `width` (clamped); deterministic; printed directly (clap reflows
    /// multi-line ANSI art). A static surface, not the live launch rain — which
    /// the Launch Progress TUI owns.
    #[must_use]
    pub fn help_banner(width: u16) -> String {
        const H: usize = 13;
        // Rows over which the rain dims to black as it nears the logo band.
        const FADE_ROWS: usize = 4;
        const POOL: &[u8] =
            b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz@#$%&*<>{}[]|/\\~";
        let w = usize::from(width.saturating_sub(4)).clamp(48, 76);

        let pill = "\x1b[1m\x1b[48;2;0;255;65m\x1b[38;2;0;0;0m jackin\x1b[38;2;255;255;255m❯\x1b[38;2;0;0;0m \x1b[0m";
        const PILL_W: usize = 9; // visual width of " jackin❯ "
        let byline = "by tailrocks";
        let byl_w = byline.len();
        let lock_row = H / 2 - 1;
        let byl_row = H / 2 + 1;
        let lc = w.saturating_sub(PILL_W) / 2;
        let bc = w.saturating_sub(byl_w) / 2;
        // Clear halo around the lockup so the rain never overprints the mark.
        let box_top = lock_row - 1;
        let box_bot = byl_row + 1;
        let box_lo = lc.min(bc).saturating_sub(3);
        let box_hi = (lc + PILL_W).max(bc + byl_w) + 3;

        // Same age -> colour ramp as the launch rain's `age_to_color`.
        let age_to_rgb = |age: u16| -> Option<Rgb> {
            match age {
                0 => Some(crate::RAIN_HEAD),
                1..=2 => Some(crate::RAIN_FRESH),
                3..=5 => Some(crate::RAIN_BODY),
                6..=10 => Some(crate::RAIN_MID),
                11..=16 => Some(crate::RAIN_DIM),
                17..=24 => Some(crate::RAIN_DARK),
                _ => None,
            }
        };
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
                // Centred green-block lockup + byline.
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
                // Per-column drop: white head + trail above = vertical rain.
                let col_seed = xorshift((c as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x5EED);
                let lit = if col_seed % 100 < 55 {
                    let head = (col_seed >> 8) % (H as u64 + 6);
                    let fade = 1 + (col_seed >> 24) % 3;
                    if (r as u64) <= head {
                        // Trail tops out at 24 age units, so this fits u16.
                        age_to_rgb(((head - r as u64) * fade) as u16)
                    } else {
                        None
                    }
                } else {
                    None
                };
                match lit {
                    Some(rgb) => {
                        // Dim toward the centre logo band so the rain dissolves
                        // into the mark, as the launch rain does into the bar.
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

    /// Minimum terminal width (columns) for [`help_banner`]; narrower terminals
    /// show the one-line [`BRAND_BANNER`] pill instead.
    pub const HELP_BANNER_MIN_COLS: u16 = 60;

    /// Build a foreground SGR for a shared RGB token.
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
            (180, 255, 180) => "\x1b[38;2;180;255;180m", // ACTION_ACCENT
            _ => "",
        }
    }

    /// Build a background SGR for a shared RGB token.
    pub const fn rgb_bg(rgb: Rgb) -> &'static str {
        match (rgb.r, rgb.g, rgb.b) {
            (0, 255, 65) => "\x1b[48;2;0;255;65m",
            (42, 42, 42) => "\x1b[48;2;42;42;42m",
            (255, 255, 255) => "\x1b[48;2;255;255;255m",
            (0, 0, 0) => "\x1b[48;2;0;0;0m",
            _ => "",
        }
    }

    /// Truecolor foreground SGR for an arbitrary RGB value.
    ///
    /// The `const` `rgb_fg` above returns a `&'static str` and so must match a
    /// fixed allowlist. Render code that picks a color at runtime must use this
    /// instead: a `Color::Rgb` the allowlist happens not to cover would
    /// otherwise lose color on the frame that first paints it.
    #[must_use]
    pub fn rgb_fg_dyn(rgb: Rgb) -> String {
        format!("\x1b[38;2;{};{};{}m", rgb.r, rgb.g, rgb.b)
    }

    /// Truecolor background SGR for an arbitrary RGB value. Runtime counterpart
    /// to the `const` `rgb_bg`; see [`rgb_fg_dyn`] for why render-time callers
    /// must never route through the panicking `const` allowlist.
    #[must_use]
    pub fn rgb_bg_dyn(rgb: Rgb) -> String {
        format!("\x1b[48;2;{};{};{}m", rgb.r, rgb.g, rgb.b)
    }

    /// OSC 52 clipboard-write sequence. Targets the system clipboard (`c`)
    /// and uses BEL termination, which is accepted by Ghostty, Kitty, iTerm2,
    /// Alacritty, and `WezTerm`. (GNOME Terminal / VTE has historically required
    /// ST `\x1b\\` for OSC 52 — keep it off the BEL-supported list until a
    /// specific VTE version can be cited.)
    #[must_use]
    pub fn encode_osc52_clipboard_write(payload: &str) -> Vec<u8> {
        let encoded = BASE64.encode(payload.as_bytes());
        let mut out = Vec::with_capacity(8 + encoded.len());
        out.extend_from_slice(b"\x1b]52;c;");
        out.extend_from_slice(encoded.as_bytes());
        out.extend_from_slice(b"\x07");
        out
    }

    /// Open an OSC 8 hyperlink for subsequent terminal text. Call
    /// [`emit_osc8_close`] after writing the linked text.
    pub fn emit_osc8_open(buf: &mut Vec<u8>, href: &str) {
        buf.extend_from_slice(b"\x1b]8;;");
        buf.extend_from_slice(href.as_bytes());
        buf.extend_from_slice(b"\x1b\\");
    }

    /// Close the active OSC 8 hyperlink.
    pub fn emit_osc8_close(buf: &mut Vec<u8>) {
        buf.extend_from_slice(b"\x1b]8;;\x1b\\");
    }

    /// Emit a `1;1`-origin cursor positioning sequence.
    pub fn move_to(buf: &mut Vec<u8>, row: u16, col: u16) {
        let _unused = write!(buf, "\x1b[{};{}H", row + 1, col + 1);
    }

    /// Emit an SGR for a foreground RGB triple.
    pub fn fg(buf: &mut Vec<u8>, rgb: Rgb) {
        let _unused = write!(buf, "\x1b[38;2;{};{};{}m", rgb.r, rgb.g, rgb.b);
    }

    /// Emit an SGR for a background RGB triple.
    pub fn bg(buf: &mut Vec<u8>, rgb: Rgb) {
        let _unused = write!(buf, "\x1b[48;2;{};{};{}m", rgb.r, rgb.g, rgb.b);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerShape {
    Default,
    Pointer,
    Text,
    EwResize,
    NsResize,
    Grabbing,
}

impl PointerShape {
    #[must_use]
    pub const fn as_osc22_name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Pointer => "pointer",
            Self::Text => "text",
            Self::EwResize => "ew-resize",
            Self::NsResize => "ns-resize",
            Self::Grabbing => "grabbing",
        }
    }
}

#[must_use]
pub const fn clickable_pointer_shape(clickable: bool) -> PointerShape {
    if clickable {
        PointerShape::Pointer
    } else {
        PointerShape::Default
    }
}

#[must_use]
pub fn osc22_pointer_shape(shape: PointerShape) -> String {
    format!("\x1b]22;{}\x1b\\", shape.as_osc22_name())
}

#[cfg(test)]
mod tests;
