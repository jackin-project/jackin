//! Shared TUI palette and tab-strip pattern used by both jackin's
//! ratatui-based console (`src/console/`) and the in-container
//! multiplexer (`crates/jackin-container/`). The two consumers
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
/// jackin-container both follow this spacing.
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
#[must_use]
pub fn lay_out_tabs<'a>(labels: &[(&'a str, bool)], start_col: u16) -> Vec<TabCell<'a>> {
    let mut col = start_col;
    let mut out = Vec::with_capacity(labels.len());
    for &(label, active) in labels {
        let label_cols = label.chars().count() as u16;
        let cell_cols = label_cols + 2; // " label "
        out.push(TabCell {
            label,
            active,
            start_col: col,
            cell_cols,
        });
        col = col + cell_cols + TAB_GAP;
    }
    out
}

/// Cross-surface single-line text-input model. Holds the buffer,
/// cursor position (in bytes), an optional max length, and an
/// optional forbidden set used for duplicate detection. Pure data +
/// pure-Rust methods — no ratatui, no crossterm — so the same struct
/// can drive ratatui-rendered modals in the console TUI and ANSI
/// modals in jackin-container.
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
