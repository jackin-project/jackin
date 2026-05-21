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

#[cfg(test)]
mod tests {
    use super::*;

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
