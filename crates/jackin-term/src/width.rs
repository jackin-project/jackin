//! Stable virtual-terminal display-width contract.
//!
//! This module is the `jackin-term` authority for agent-visible cell width.
//! Ratatui may be cross-checked in tests, but it is not a runtime dependency of
//! the terminal model.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{Attrs, Color, UnderlineStyle};

/// Stable per-session terminal profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualTerminalProfile {
    /// Identifier for the width-table source (crate + version), not a Unicode
    /// standard version string.
    pub unicode_version: &'static str,
    /// Intended flag for whether mode-2027 grapheme-cluster width is active.
    /// Currently inert — `cluster_width`/`display_width` do not consult it yet;
    /// `false` is the legacy-cell-width default and forward contract. The DECRQM
    /// reply value itself is owned by `decrqm_mode_2027_status`.
    pub grapheme_cluster_width_mode: bool,
    /// Whether East Asian Ambiguous code points are treated as two columns.
    pub ambiguous_width_is_wide: bool,
    pub decrqm_mode_2027_status: u16,
    pub default_reported_fg: (u8, u8, u8),
    pub default_reported_bg: (u8, u8, u8),
    pub agent_term: &'static str,
    pub agent_colorterm: &'static str,
    pub osc8_policy: Osc8Policy,
    pub supported_sgr: SupportedSgr,
}

impl Default for VirtualTerminalProfile {
    fn default() -> Self {
        Self {
            unicode_version: "unicode-width 0.2",
            grapheme_cluster_width_mode: false,
            ambiguous_width_is_wide: false,
            decrqm_mode_2027_status: 0,
            default_reported_fg: (0xe6, 0xe6, 0xe6),
            default_reported_bg: (0x00, 0x00, 0x00),
            agent_term: "xterm-256color",
            agent_colorterm: "truecolor",
            osc8_policy: Osc8Policy::ModelMetadata,
            supported_sgr: SupportedSgr {
                bold: true,
                dim: true,
                italic: true,
                underline: true,
                underline_style: true,
                underline_color: true,
                inverse: true,
                strikethrough: true,
                blink: true,
                conceal: true,
                overline: true,
                color_256: true,
                truecolor: true,
            },
        }
    }
}

/// How OSC 8 hyperlinks are handled. Named `Osc8Policy` (not `OscPolicy`) to
/// avoid colliding with the capsule's broader `session::OscPolicy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Osc8Policy {
    /// Hyperlinks are modeled as cell metadata rather than passed through raw.
    ModelMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedSgr {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub underline_style: bool,
    pub underline_color: bool,
    pub inverse: bool,
    pub strikethrough: bool,
    pub blink: bool,
    pub conceal: bool,
    pub overline: bool,
    pub color_256: bool,
    pub truecolor: bool,
}

impl VirtualTerminalProfile {
    /// Width of one printable scalar before it joins an existing cluster.
    #[must_use]
    pub fn char_width(self, ch: char) -> u16 {
        UnicodeWidthChar::width(ch).unwrap_or(1).min(2) as u16
    }

    /// Width of one accepted grapheme-ish cluster in the virtual terminal.
    #[must_use]
    pub fn cluster_width(self, cluster: &str) -> u16 {
        display_width(cluster)
    }

    #[must_use]
    pub fn decrqm_status(self, mode: u16) -> u16 {
        if mode == 2027 {
            self.decrqm_mode_2027_status
        } else {
            0
        }
    }

    #[must_use]
    pub fn default_reported_color(self, code: u8) -> Option<(u8, u8, u8)> {
        match code {
            10 => Some(self.default_reported_fg),
            11 => Some(self.default_reported_bg),
            _ => None,
        }
    }

    #[must_use]
    pub fn attrs_supported(self, attrs: &Attrs) -> bool {
        let sgr = self.supported_sgr;
        (sgr.color_256
            || sgr.truecolor
            || (attrs.foreground == Color::Default && attrs.background == Color::Default))
            && (!attrs.bold || sgr.bold)
            && (!attrs.dim || sgr.dim)
            && (!attrs.italic || sgr.italic)
            && (attrs.underline_style == UnderlineStyle::None || sgr.underline)
            && (!attrs.strikethrough || sgr.strikethrough)
            && (!(attrs.slow_blink || attrs.rapid_blink) || sgr.blink)
            && (!attrs.conceal || sgr.conceal)
            && (!attrs.overline || sgr.overline)
    }
}

/// Width of one accepted cluster in the jackin❯ virtual terminal profile.
#[must_use]
pub fn display_width(cluster: &str) -> u16 {
    if cluster.is_empty() {
        return 0;
    }

    (UnicodeWidthStr::width(cluster) as u16)
        .saturating_add(count_halfwidth_katakana_voicing_marks(cluster))
        .min(2)
}

fn count_halfwidth_katakana_voicing_marks(cluster: &str) -> u16 {
    cluster
        .chars()
        .filter(|ch| matches!(ch, '\u{ff9e}' | '\u{ff9f}'))
        .count() as u16
}

#[cfg(test)]
mod tests;
