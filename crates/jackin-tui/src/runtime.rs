//! Shared runtime contracts for Ratatui-style update loops.

/// Whether applying a message changed visible state and should schedule a draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum Dirty {
    Clean,
    Redraw,
}

impl Dirty {
    #[must_use]
    pub const fn is_dirty(self) -> bool {
        matches!(self, Self::Redraw)
    }

    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Redraw, _) | (_, Self::Redraw) => Self::Redraw,
            (Self::Clean, Self::Clean) => Self::Clean,
        }
    }
}
