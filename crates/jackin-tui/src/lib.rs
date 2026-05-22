//! Shared TUI palette and tab-strip pattern used by both jackin's
//! ratatui-based console (`src/console/`) and the in-container
//! multiplexer (`crates/jackin-capsule/`). The two consumers
//! produce different output formats — ratatui `Color` widgets vs
//! raw ANSI bytes — so this crate keeps the cross-cutting bits at
//! the lowest common denominator: plain RGB triples for colours and
//! a struct describing a single tab cell. Each consumer adapts the
//! struct to its own renderer.
//!
//! Adding direct renderer-specific code here would force a
//! dependency choice (ratatui vs raw ANSI) that doesn't belong in a
//! shared crate. Keep the surface narrow.

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

/// `--jk-brand` — the bright phosphor green used for selection
/// highlights, the row-0 brand pill, and live indicators.
pub const PHOSPHOR_GREEN: Rgb = Rgb::new(0, 255, 65);

/// Mid-green used for inactive tab labels, dim labels, and "Dyn"
/// footer text in the console.
pub const PHOSPHOR_DIM: Rgb = Rgb::new(0, 140, 30);

/// Dark green used for panel borders and dot separators.
pub const PHOSPHOR_DARK: Rgb = Rgb::new(0, 80, 18);

/// Pure black background for modal dialogs that need to mask the
/// agent's content behind the overlay.
pub const BLACK: Rgb = Rgb::new(0, 0, 0);

/// White used for titles, hotkey glyphs, and the active-tab underline.
pub const WHITE: Rgb = Rgb::new(255, 255, 255);

/// Almost-invisible dim background for the input band inside a
/// text-input dialog. Picked so the input region is visible even when
/// empty without competing with the dialog's PHOSPHOR_DARK border.
/// Used by the host TUI's `text_input` widget and the
/// `jackin-capsule` rename dialog so both surfaces share the same
/// "this is where you type" cue.
pub const INPUT_BG_DIM: Rgb = Rgb::new(20, 24, 22);

/// Per-tab descriptor consumed by both ratatui and ANSI tab
/// renderers. `cell_cols` is the number of display columns the cell
/// occupies including its left/right padding spaces.
#[derive(Debug, Clone)]
pub struct TabCell<'a> {
    pub label: &'a str,
    pub active: bool,
    /// 0-based column index where this cell's leftmost space starts.
    pub start_col: u16,
    /// Display column width of the cell (`label_cols + 2` padding).
    pub cell_cols: u16,
}

/// Single space between adjacent tab cells. Console TUI and
/// jackin-capsule both follow this spacing.
pub const TAB_GAP: u16 = 1;

/// Title-case display name for an agent slug. Mirrors the console
/// TUI's `agent_picker_label` so both surfaces use the same casing.
/// Returns `None` for unrecognised slugs so callers can fall back to
/// the raw slug rather than silently displaying a wrong label.
#[must_use]
pub fn agent_display_name(slug: &str) -> Option<&'static str> {
    match slug {
        "claude" => Some("Claude"),
        "codex" => Some("Codex"),
        "amp" => Some("Amp"),
        "kimi" => Some("Kimi"),
        "opencode" => Some("OpenCode"),
        _ => None,
    }
}

/// Build a row of `TabCell` descriptors from `(label, active)` pairs,
/// starting at `start_col`. Used by both consumers to compute
/// click-region bounds and to know where to paint the active-tab
/// underline.
///
/// Column width is measured with `unicode-width`. Plain `.chars().count()`
/// silently counts wide glyphs (CJK, emoji) as 1 column and combining
/// marks as N columns instead of the 2 / 0 cells they actually occupy
/// — every downstream click hit-test and underline placement then drifts
/// by however many wide/combining chars sit before the active tab.
#[must_use]
pub fn lay_out_tabs<'a>(labels: &[(&'a str, bool)], start_col: u16) -> Vec<TabCell<'a>> {
    use unicode_width::UnicodeWidthStr;
    let mut col = start_col;
    let mut out = Vec::with_capacity(labels.len());
    for &(label, active) in labels {
        let label_cols = u16::try_from(UnicodeWidthStr::width(label)).unwrap_or(u16::MAX);
        let cell_cols = label_cols.saturating_add(2); // " label "
        out.push(TabCell {
            label,
            active,
            start_col: col,
            cell_cols,
        });
        col = col.saturating_add(cell_cols).saturating_add(TAB_GAP);
    }
    out
}

/// Cross-surface single-line text-input model. Holds the buffer,
/// cursor position (in bytes), an optional max length, and an
/// optional forbidden set used for duplicate detection. Pure data +
/// pure-Rust methods — no ratatui, no crossterm — so the same struct
/// can drive ratatui-rendered modals in the console TUI and ANSI
/// modals in jackin-capsule.
///
/// Cursor is a byte offset to keep `insert_char` cheap; the public
/// edit operations advance/retreat by one char each so multi-byte
/// glyphs are not split.
#[derive(Debug, Clone)]
pub struct TextField {
    value: String,
    cursor: usize,
    max_chars: Option<usize>,
    forbidden: Vec<String>,
    allow_empty: bool,
}

impl Default for TextField {
    fn default() -> Self {
        Self::new("")
    }
}

impl TextField {
    pub fn new(initial: impl Into<String>) -> Self {
        let value: String = initial.into();
        let cursor = value.len();
        Self {
            value,
            cursor,
            max_chars: None,
            forbidden: Vec::new(),
            allow_empty: false,
        }
    }

    pub fn with_max_chars(mut self, n: usize) -> Self {
        self.max_chars = Some(n);
        self
    }

    pub fn with_forbidden(mut self, forbidden: Vec<String>) -> Self {
        self.forbidden = forbidden;
        self
    }

    pub fn with_allow_empty(mut self, allow: bool) -> Self {
        self.allow_empty = allow;
        self
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn trimmed_value(&self) -> String {
        self.value.trim().to_string()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn len_chars(&self) -> usize {
        self.value.chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Insert a single character at the cursor. Rejects the insert
    /// when `max_chars` is set and the buffer is already full. Control
    /// chars (NUL, ESC, DEL, etc.) are silently dropped — callers
    /// should pre-filter to printable input.
    pub fn insert_char(&mut self, c: char) {
        if c.is_control() {
            return;
        }
        if let Some(max) = self.max_chars
            && self.len_chars() >= max
        {
            return;
        }
        self.value.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Remove the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev_char_start = self.value[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.value.replace_range(prev_char_start..self.cursor, "");
        self.cursor = prev_char_start;
    }

    /// True when the trimmed value matches `forbidden` (non-empty).
    pub fn is_duplicate(&self) -> bool {
        let v = self.trimmed_value();
        !v.is_empty() && self.forbidden.iter().any(|f| f == &v)
    }

    pub fn is_valid(&self) -> bool {
        let v = self.trimmed_value();
        let empty_ok = self.allow_empty || !v.is_empty();
        empty_ok && !self.forbidden.iter().any(|f| f == &v)
    }
}

/// Shorten an absolute path by replacing the operator's `$HOME`
/// prefix with `~`. Shared between the in-container multiplexer's
/// pane-box title and the console TUI's path display so both
/// surfaces collapse the home directory the same way.
#[must_use]
pub fn shorten_home(path: &str) -> String {
    let Some(home) = std::env::var_os("HOME") else {
        return path.to_string();
    };
    let home = home.to_string_lossy().into_owned();
    if home.is_empty() || !path.starts_with(&home) {
        return path.to_string();
    }
    let rest = &path[home.len()..];
    // Only collapse when the next character after `$HOME` is a path
    // separator (or end of string). Otherwise `/Users/alice.notmine`
    // would incorrectly compact to `~.notmine`.
    if rest.is_empty() || rest.starts_with('/') {
        format!("~{rest}")
    } else {
        path.to_string()
    }
}

/// Computed thumb position + length for a vertical scrollbar. Shared
/// math between the host TUI's ratatui-based scrollable blocks and
/// the in-container multiplexer's raw-ANSI overlay, so both surfaces
/// pick the same thumb size and the same proportional position for
/// the same (track_rows, content_filled, offset) triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerticalThumb {
    /// 0-based row inside the track where the thumb starts.
    pub thumb_top: u16,
    /// Number of rows the thumb spans. Always ≥ 1 when there is any
    /// scrollback; clamps to `track_rows` when nearly everything is
    /// off-screen.
    pub thumb_rows: u16,
}

/// Compute thumb geometry for a vertical scrollbar.
///
/// - `track_rows`: how many rows the scrollbar track spans
///   (typically the pane's interior height, excluding the top and
///   bottom border rows).
/// - `filled`: lines of scrollback currently held beyond the visible
///   region.
/// - `offset`: how many lines the operator has scrolled back from
///   the live tail. `0` parks the thumb at the bottom of the track;
///   `filled` parks it at the top.
///
/// Returns `None` when there is no thumb to draw (`track_rows == 0`
/// or `filled == 0`).
#[must_use]
pub fn vertical_thumb(track_rows: u16, filled: usize, offset: usize) -> Option<VerticalThumb> {
    if track_rows == 0 || filled == 0 {
        return None;
    }
    let track = track_rows as usize;
    let total = filled + track;
    let thumb_rows = ((track * track) / total).max(1).min(track);
    let unscrolled_room = track - thumb_rows;
    let thumb_top_from_bottom = (offset * unscrolled_room).checked_div(filled).unwrap_or(0);
    let thumb_top = unscrolled_room.saturating_sub(thumb_top_from_bottom);
    Some(VerticalThumb {
        thumb_top: thumb_top as u16,
        thumb_rows: thumb_rows as u16,
    })
}

/// Shared ANSI helpers + a centred text-input dialog renderer. The
/// host TUI uses ratatui directly; the in-container multiplexer
/// emits raw ANSI. Keeping the visual recipe (border style, title
/// formatting, dim-bg input band, inverted cursor block, footer hint
/// placement) in one place stops the two surfaces from drifting
/// apart when one side picks up a tweak the other forgets.
pub mod ansi {
    use super::{INPUT_BG_DIM, PHOSPHOR_DARK, PHOSPHOR_GREEN, Rgb, WHITE};
    use std::io::Write as _;

    /// Pure-black background for modal overlays. Matches the
    /// `BG_DARK` constant the in-container dialog renderer uses.
    pub const BG_DARK: &str = "\x1b[48;2;0;0;0m";
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const INVERSE: &str = "\x1b[7m";

    /// Emit a `1;1`-origin cursor positioning sequence.
    pub fn move_to(buf: &mut Vec<u8>, row: u16, col: u16) {
        let _ = write!(buf, "\x1b[{};{}H", row + 1, col + 1);
    }

    /// Emit an SGR for a foreground RGB triple.
    pub fn fg(buf: &mut Vec<u8>, rgb: Rgb) {
        let _ = write!(buf, "\x1b[38;2;{};{};{}m", rgb.r, rgb.g, rgb.b);
    }

    /// Emit an SGR for a background RGB triple.
    pub fn bg(buf: &mut Vec<u8>, rgb: Rgb) {
        let _ = write!(buf, "\x1b[48;2;{};{};{}m", rgb.r, rgb.g, rgb.b);
    }

    /// Centred text-input dialog matching the host TUI's
    /// `text_input` widget. Dialog spans 60% of `term_cols` (clamped
    /// to `[40, 100]`) and is 5 rows tall: top border, pad, input
    /// band, pad, bottom border.
    ///
    /// `cursor_col` is the byte offset into `value` where the caret
    /// should sit; multi-byte glyphs are not split (only ASCII cases
    /// are required by the rename modal today).
    pub fn render_text_input_dialog(
        buf: &mut Vec<u8>,
        term_rows: u16,
        term_cols: u16,
        label: &str,
        value: &str,
        cursor_byte: usize,
    ) -> TextInputDialogRect {
        let width = (term_cols * 60 / 100).clamp(40, 100);
        let height: u16 = 5;
        let row = term_rows.saturating_sub(height) / 2;
        let col = term_cols.saturating_sub(width) / 2;

        // Top border with ` Label ` callout in WHITE+BOLD.
        move_to(buf, row, col);
        buf.extend_from_slice(BG_DARK.as_bytes());
        fg(buf, PHOSPHOR_DARK);
        buf.extend_from_slice("┌─ ".as_bytes());
        fg(buf, WHITE);
        buf.extend_from_slice(BOLD.as_bytes());
        buf.extend_from_slice(label.as_bytes());
        buf.extend_from_slice(RESET.as_bytes());
        buf.extend_from_slice(BG_DARK.as_bytes());
        fg(buf, PHOSPHOR_DARK);
        buf.push(b' ');
        let consumed = 3 /* "┌─ " */ + label.chars().count() as u16 + 1 /* " " */;
        for _ in consumed..(width - 1) {
            buf.extend_from_slice("─".as_bytes());
        }
        buf.extend_from_slice("┐".as_bytes());

        // Pad row above input.
        move_to(buf, row + 1, col);
        buf.extend_from_slice(BG_DARK.as_bytes());
        fg(buf, PHOSPHOR_DARK);
        buf.extend_from_slice("│".as_bytes());
        for _ in 1..(width - 1) {
            buf.push(b' ');
        }
        buf.extend_from_slice("│".as_bytes());

        // Input row: side borders, then a dim-BG band that spans
        // (inner_width - 2) cells, with a 1-cell pad on each side so
        // the value doesn't touch the band's left edge.
        move_to(buf, row + 2, col);
        buf.extend_from_slice(BG_DARK.as_bytes());
        fg(buf, PHOSPHOR_DARK);
        buf.extend_from_slice("│".as_bytes());
        buf.push(b' ');
        bg(buf, INPUT_BG_DIM);
        let band_cols = (width as usize).saturating_sub(4);
        // Paint the dim band.
        for _ in 0..band_cols {
            buf.push(b' ');
        }
        // Reposition to the band's start to overlay the value + caret.
        move_to(buf, row + 2, col + 2);
        bg(buf, INPUT_BG_DIM);
        fg(buf, WHITE);
        let cursor_byte = cursor_byte.min(value.len());
        let (before, after) = value.split_at(cursor_byte);
        buf.extend_from_slice(before.as_bytes());
        // Caret as inverse single space (or the next char rendered
        // inverted); when `after` is empty, paint an inverse space.
        buf.extend_from_slice(INVERSE.as_bytes());
        fg(buf, PHOSPHOR_GREEN);
        if let Some(c) = after.chars().next() {
            let mut b = [0u8; 4];
            let s = c.encode_utf8(&mut b);
            buf.extend_from_slice(s.as_bytes());
            buf.extend_from_slice(RESET.as_bytes());
            buf.extend_from_slice(BG_DARK.as_bytes());
            bg(buf, INPUT_BG_DIM);
            fg(buf, WHITE);
            let tail = &after[c.len_utf8()..];
            buf.extend_from_slice(tail.as_bytes());
        } else {
            buf.push(b' ');
            buf.extend_from_slice(RESET.as_bytes());
            buf.extend_from_slice(BG_DARK.as_bytes());
            bg(buf, INPUT_BG_DIM);
        }
        // Restore band style + right pad + right border.
        buf.extend_from_slice(RESET.as_bytes());
        buf.extend_from_slice(BG_DARK.as_bytes());
        fg(buf, PHOSPHOR_DARK);
        move_to(buf, row + 2, col + width - 2);
        buf.push(b' ');
        buf.extend_from_slice("│".as_bytes());

        // Pad row below input.
        move_to(buf, row + 3, col);
        buf.extend_from_slice(BG_DARK.as_bytes());
        fg(buf, PHOSPHOR_DARK);
        buf.extend_from_slice("│".as_bytes());
        for _ in 1..(width - 1) {
            buf.push(b' ');
        }
        buf.extend_from_slice("│".as_bytes());

        // Bottom border.
        move_to(buf, row + height - 1, col);
        buf.extend_from_slice(BG_DARK.as_bytes());
        fg(buf, PHOSPHOR_DARK);
        buf.extend_from_slice("└".as_bytes());
        for _ in 1..(width - 1) {
            buf.extend_from_slice("─".as_bytes());
        }
        buf.extend_from_slice("┘".as_bytes());
        buf.extend_from_slice(RESET.as_bytes());

        TextInputDialogRect {
            row,
            col,
            width,
            height,
        }
    }

    /// Returned by `render_text_input_dialog` so callers can hit-test
    /// clicks against the dialog box.
    #[derive(Debug, Clone, Copy)]
    pub struct TextInputDialogRect {
        pub row: u16,
        pub col: u16,
        pub width: u16,
        pub height: u16,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_field_insert_appends() {
        let mut f = TextField::new("");
        f.insert_char('a');
        f.insert_char('b');
        assert_eq!(f.value(), "ab");
        assert_eq!(f.cursor(), 2);
    }

    #[test]
    fn text_field_backspace_removes_one_char() {
        let mut f = TextField::new("abc");
        f.backspace();
        assert_eq!(f.value(), "ab");
    }

    #[test]
    fn text_field_max_chars_caps_buffer() {
        let mut f = TextField::new("").with_max_chars(2);
        f.insert_char('a');
        f.insert_char('b');
        f.insert_char('c');
        assert_eq!(f.value(), "ab");
    }

    #[test]
    fn text_field_duplicate_detection_trims() {
        let f = TextField::new("  foo  ").with_forbidden(vec!["foo".into()]);
        assert!(f.is_duplicate());
    }

    #[test]
    fn text_field_is_valid_requires_non_empty_by_default() {
        let f = TextField::new("");
        assert!(!f.is_valid());
        let f = f.with_allow_empty(true);
        assert!(f.is_valid());
    }

    #[test]
    fn shorten_home_returns_path_when_no_match() {
        // Use the actual `HOME` from the test environment without
        // mutating it. Anything not starting with `$HOME` is
        // returned unchanged, which is the only branch we can
        // verify reliably without an `unsafe` env-var write (the
        // crate's lints forbid `unsafe`).
        let home = std::env::var("HOME").unwrap_or_default();
        let alien = if home == "/" {
            "etc/hosts".to_string()
        } else {
            format!("{home}.notmine")
        };
        assert_eq!(shorten_home(&alien), alien);
    }

    #[test]
    fn text_field_control_chars_are_ignored() {
        let mut f = TextField::new("");
        f.insert_char('\n');
        f.insert_char('\x1b');
        assert!(f.is_empty());
    }

    #[test]
    fn lay_out_tabs_packs_cells_with_single_gap() {
        let cells = lay_out_tabs(&[("General", true), ("Mounts", false)], 0);
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].start_col, 0);
        assert_eq!(cells[0].cell_cols, 9); // " General "
        assert!(cells[0].active);
        // Second tab starts after first cell + single-column gap.
        assert_eq!(cells[1].start_col, 9 + 1);
        assert_eq!(cells[1].cell_cols, 8); // " Mounts "
        assert!(!cells[1].active);
    }
}
