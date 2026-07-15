//! jackin-tui: shared TUI widgets, theme, and render helpers.
//!
//! **Architecture Invariant:** T1.
//! Entry point: [`theme`] — shared TUI palette constants and style helpers.

pub mod animation;
pub mod ansi_text;
pub mod components;
pub mod geometry;
pub mod host_colors;
pub mod keymap;
pub mod output;
pub mod ownership;
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
pub mod ansi;

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
