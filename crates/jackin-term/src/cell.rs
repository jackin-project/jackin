// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Cell type for the `DamageGrid` implementation.
//!
//! Phase 2: representation matches the capsule terminal-model surface.
//! Phase 4: `Cell::contents` uses `CompactString` (≤24 bytes inline, no heap
//! alloc for ASCII + most Unicode grapheme clusters). This eliminates the
//! per-cell `String::to_string()` alloc storm in the focused-pane render path.
//! The public `contents() -> &str` API is unchanged.

use compact_str::CompactString;

/// Color representation used by the owned terminal model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    /// Terminal default foreground or background.
    #[default]
    Default,
    /// 256-color palette index.
    Idx(u8),
    /// True-color RGB.
    Rgb(u8, u8, u8),
}

/// Cell attributes (a subset of SGR properties the capsule reads).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Standard CSI SGR boolean attribute set (bold, italic, inverse, dim, \
              strikethrough, slow_blink, rapid_blink, conceal, overline) — nine \
              orthogonal text-attribute bits the cell writer sets/clears per \
              glyph. Named-field reads match the per-bit SGR bit-set idiom."
)]
pub struct Attrs {
    /// Foreground color.
    pub foreground: Color,
    /// Background color.
    pub background: Color,
    /// Underline color (when styled underlines are active).
    pub underline_color: Color,
    /// Underline style (none / single / double / curly / dotted / dashed).
    pub underline_style: UnderlineStyle,
    /// SGR bold.
    pub bold: bool,
    /// SGR italic.
    pub italic: bool,
    /// SGR reverse video.
    pub inverse: bool,
    /// SGR faint/dim.
    pub dim: bool,
    /// SGR strikethrough.
    pub strikethrough: bool,
    /// SGR slow blink.
    pub slow_blink: bool,
    /// SGR rapid blink.
    pub rapid_blink: bool,
    /// SGR conceal/hidden.
    pub conceal: bool,
    /// SGR overline.
    pub overline: bool,
}

/// Underline style from SGR 4 / 21 / 4:n.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UnderlineStyle {
    /// No underline.
    #[default]
    None,
    /// Single underline (SGR 4 / 4:1).
    Single,
    /// Double underline (SGR 21 / 4:2).
    Double,
    /// Curly underline (SGR 4:3).
    Curly,
    /// Dotted underline (SGR 4:4).
    Dotted,
    /// Dashed underline (SGR 4:5).
    Dashed,
}

/// OSC 8 hyperlink metadata attached to a cell span.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hyperlink {
    /// OSC 8 `id=` parameter, grouping discontiguous spans of one logical link.
    /// Parsed and snapshotted but not yet read by the emitter (which keys on
    /// `uri`); retained for the deferred span-grouping consumer.
    pub id: String,
    /// Hyperlink target URI.
    pub uri: String,
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
    /// Hyperlink id for OSC 8 regions.
    ///
    /// `0` means no hyperlink for this cell.
    pub hyperlink_id: u32,
    /// SGR attributes for this cell.
    pub attrs: Attrs,
    /// Full OSC 8 hyperlink (id + uri), when present.
    pub hyperlink: Option<Hyperlink>,
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

    /// Whether bold is set.
    pub fn bold(&self) -> bool {
        self.attrs.bold
    }

    /// Whether italic is set.
    pub fn italic(&self) -> bool {
        self.attrs.italic
    }

    /// Whether any underline style is active.
    pub fn underline(&self) -> bool {
        self.attrs.underline_style != UnderlineStyle::None
    }

    /// Whether reverse video is set.
    pub fn inverse(&self) -> bool {
        self.attrs.inverse
    }

    /// Whether dim/faint is set.
    pub fn dim(&self) -> bool {
        self.attrs.dim
    }

    /// Whether strikethrough is set.
    pub fn strikethrough(&self) -> bool {
        self.attrs.strikethrough
    }

    /// Whether slow blink is set.
    pub fn slow_blink(&self) -> bool {
        self.attrs.slow_blink
    }

    /// Whether rapid blink is set.
    pub fn rapid_blink(&self) -> bool {
        self.attrs.rapid_blink
    }

    /// Whether conceal/hidden is set.
    pub fn conceal(&self) -> bool {
        self.attrs.conceal
    }

    /// Whether overline is set.
    pub fn overline(&self) -> bool {
        self.attrs.overline
    }
}
