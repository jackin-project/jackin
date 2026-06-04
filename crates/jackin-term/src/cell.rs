//! Cell type for the `DamageGrid` implementation.
//!
//! Phase 2: representation matches the vt100 oracle coupling surface.
//! Phase 4: `Cell::contents` uses `CompactString` (≤24 bytes inline, no heap
//! alloc for ASCII + most Unicode grapheme clusters). This eliminates the
//! per-cell `String::to_string()` alloc storm in the focused-pane render path.
//! The public `contents() -> &str` API is unchanged.

use compact_str::CompactString;

/// Color representation matching the vt100 oracle coupling surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    /// 256-color palette index.
    Idx(u8),
    /// True-color RGB.
    Rgb(u8, u8, u8),
}

/// Cell attributes (a subset of SGR properties the capsule reads).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attrs {
    pub foreground: Color,
    pub background: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub dim: bool,
}

/// A single cell in the terminal grid.
///
/// `contents` is the grapheme cluster rendered at this cell position.
/// An empty string means the cell is blank (space). Wide characters occupy
/// the first column and set `is_wide`; the continuation column has empty
/// `contents` and `is_wide_continuation = true`.
///
/// Phase 4: `contents` uses `CompactString` which stores ≤24 bytes inline
/// (the common case for ASCII and most Unicode) with zero heap allocation.
/// The public `contents() -> &str` and `Cell::default()` APIs are unchanged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cell {
    /// Grapheme cluster. Empty = blank (space). Stored inline for ≤24 bytes.
    pub contents: CompactString,
    /// True for the lead column of a wide (2-col) character.
    pub is_wide: bool,
    /// True for the phantom continuation column of a wide character.
    pub is_wide_continuation: bool,
    pub attrs: Attrs,
}

impl Cell {
    /// True if the cell has non-blank content.
    pub fn has_contents(&self) -> bool {
        !self.contents.is_empty()
    }

    /// Contents as `&str`.
    pub fn contents(&self) -> &str {
        &self.contents
    }

    /// Foreground color.
    pub fn fgcolor(&self) -> Color {
        self.attrs.foreground
    }

    /// Background color.
    pub fn bgcolor(&self) -> Color {
        self.attrs.background
    }

    pub fn bold(&self) -> bool {
        self.attrs.bold
    }

    pub fn italic(&self) -> bool {
        self.attrs.italic
    }

    pub fn underline(&self) -> bool {
        self.attrs.underline
    }

    pub fn inverse(&self) -> bool {
        self.attrs.inverse
    }

    pub fn dim(&self) -> bool {
        self.attrs.dim
    }
}
