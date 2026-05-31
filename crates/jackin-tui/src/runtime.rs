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

    pub const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Redraw, _) | (_, Self::Redraw) => Self::Redraw,
            (Self::Clean, Self::Clean) => Self::Clean,
        }
    }
}

/// Marker effect type for update loops that do not produce side effects yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoEffect {}

/// Result of applying one message to a TUI model.
///
/// `dirty` tells the runtime whether to redraw. `effects` carries typed
/// side-effect requests for the app runtime to execute outside the update
/// function.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct UpdateResult<E = NoEffect> {
    dirty: Dirty,
    effects: Vec<E>,
}

impl<E> UpdateResult<E> {
    pub const fn clean() -> Self {
        Self {
            dirty: Dirty::Clean,
            effects: Vec::new(),
        }
    }

    pub const fn redraw() -> Self {
        Self {
            dirty: Dirty::Redraw,
            effects: Vec::new(),
        }
    }

    pub fn with_effect(effect: E) -> Self {
        Self {
            dirty: Dirty::Redraw,
            effects: vec![effect],
        }
    }

    pub const fn dirty(&self) -> Dirty {
        self.dirty
    }

    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty.is_dirty()
    }

    #[must_use]
    pub fn effects(&self) -> &[E] {
        &self.effects
    }

    #[must_use]
    pub fn into_effects(self) -> Vec<E> {
        self.effects
    }

    pub fn merge(mut self, other: Self) -> Self {
        self.dirty = self.dirty.merge(other.dirty);
        self.effects.extend(other.effects);
        self
    }
}
