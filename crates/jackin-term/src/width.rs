//! Stable virtual-terminal display-width contract.
//!
//! This module is the `jackin-term` authority for agent-visible cell width.
//! Ratatui may be cross-checked in tests, but it is not a runtime dependency of
//! the terminal model.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Stable per-session terminal profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualTerminalProfile {
    /// Unicode version implied by the width tables used by `unicode-width`.
    pub unicode_version: &'static str,
    /// DECRQM mode 2027 answer. `false` keeps agent apps on legacy cell widths.
    pub grapheme_cluster_width_mode: bool,
    /// Whether East Asian Ambiguous code points are treated as two columns.
    pub ambiguous_width_is_wide: bool,
}

impl Default for VirtualTerminalProfile {
    fn default() -> Self {
        Self {
            unicode_version: "unicode-width 0.2",
            grapheme_cluster_width_mode: false,
            ambiguous_width_is_wide: false,
        }
    }
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
}

/// Width of one accepted cluster in the jackin' virtual terminal profile.
#[must_use]
pub fn display_width(cluster: &str) -> u16 {
    if cluster.is_empty() {
        return 0;
    }

    let width = UnicodeWidthStr::width(cluster).min(2) as u16;
    if width == 1 && has_halfwidth_katakana_voicing_mark(cluster) {
        2
    } else {
        width
    }
}

fn has_halfwidth_katakana_voicing_mark(cluster: &str) -> bool {
    cluster
        .chars()
        .any(|ch| matches!(ch, '\u{ff9e}' | '\u{ff9f}'))
}

#[cfg(test)]
mod tests;
