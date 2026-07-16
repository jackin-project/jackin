// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Cross-surface jackin❯ application adapter contracts.
//!
//! `TermRock` owns terminal primitives and its optional closure runner. jackin❯
//! owns its domain effects, external subscriptions, existing surface event
//! loops, and the small render adapters shared by those loops.

use ratatui::{CompletedFrame, Frame, Terminal, backend::Backend, layout::Rect};

mod focus;
mod modal_flow;

pub use focus::{SurfaceFocus, SurfaceFocusTarget};
pub use modal_flow::ModalFlow;

/// The result of polling a jackin❯-owned external value source once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionPoll<Event> {
    /// A value is ready.
    Ready(Event),
    /// The source has no value yet.
    Pending,
    /// The source cannot produce another value.
    Closed,
}

/// Non-blocking source of application-owned values.
pub trait Subscription {
    /// Value produced by the source.
    type Output;

    /// Poll the source once without blocking.
    fn poll_next(&mut self) -> SubscriptionPoll<Self::Output>;
}

/// Whether an application update requires another frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dirty {
    /// No frame is required.
    Clean,
    /// Render another frame.
    Redraw,
}

impl Dirty {
    /// Whether another frame is required.
    #[must_use]
    pub const fn is_dirty(self) -> bool {
        matches!(self, Self::Redraw)
    }

    /// Combine two redraw decisions.
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        if self.is_dirty() || other.is_dirty() {
            Self::Redraw
        } else {
            Self::Clean
        }
    }
}

/// Uninhabited effect type for update paths without effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoEffect {}

/// Product redraw decision and effects produced by one update.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct UpdateResult<Effect = NoEffect> {
    dirty: Dirty,
    effects: Vec<Effect>,
}

impl<Effect> UpdateResult<Effect> {
    /// Produce no effects and request no frame.
    pub const fn clean() -> Self {
        Self {
            dirty: Dirty::Clean,
            effects: Vec::new(),
        }
    }

    /// Produce no effects and request another frame.
    pub const fn redraw() -> Self {
        Self {
            dirty: Dirty::Redraw,
            effects: Vec::new(),
        }
    }

    /// Request another frame with one product effect.
    pub fn with_effect(effect: Effect) -> Self {
        Self {
            dirty: Dirty::Redraw,
            effects: vec![effect],
        }
    }

    /// Combined redraw decision.
    pub const fn dirty(&self) -> Dirty {
        self.dirty
    }

    /// Whether another frame is required.
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty.is_dirty()
    }

    /// Product effects emitted by this update.
    #[must_use]
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }

    /// Combine redraw state and preserve both effect sequences.
    pub fn merge(mut self, other: Self) -> Self {
        self.dirty = self.dirty.merge(other.dirty);
        self.effects.extend(other.effects);
        self
    }
}

/// Product input-to-message adapter used by an existing surface loop.
pub trait Component<Event, Message> {
    /// Translate one input into an optional product message.
    fn handle_event(&mut self, event: &Event) -> Option<Message>;
}

/// Product model-to-frame adapter used by an existing surface loop.
pub trait View<Model> {
    /// Render the model into the supplied product-owned area.
    fn render(&self, model: &Model, frame: &mut Frame<'_>, area: Rect);
}

/// Render a product view and its surface-local overlay in one terminal draw.
pub fn drive_frame<'a, B, Model, V, F>(
    terminal: &'a mut Terminal<B>,
    view: &V,
    model: &Model,
    area: Rect,
    overlay: F,
) -> Result<CompletedFrame<'a>, B::Error>
where
    B: Backend,
    V: View<Model>,
    F: FnOnce(&mut Frame<'_>),
{
    terminal.draw(|frame| {
        view.render(model, frame, area);
        overlay(frame);
    })
}

/// Render through a surface-provided callback in one terminal draw.
pub fn drive_render<B, F>(
    terminal: &mut Terminal<B>,
    render: F,
) -> Result<CompletedFrame<'_>, B::Error>
where
    B: Backend,
    F: FnOnce(&mut Frame<'_>),
{
    terminal.draw(render)
}

#[cfg(test)]
mod tests;
