// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::{CompletedFrame, Frame, Terminal, backend::Backend, layout::Rect};

mod focus;

pub use focus::{SurfaceFocus, SurfaceFocusTarget};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionPoll<Event> {
    Ready(Event),
    Pending,
    Closed,
}

pub trait Subscription {
    type Output;

    fn poll_next(&mut self) -> SubscriptionPoll<Self::Output>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        if self.is_dirty() || other.is_dirty() {
            Self::Redraw
        } else {
            Self::Clean
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoEffect {}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct UpdateResult<Effect = NoEffect> {
    dirty: Dirty,
    effects: Vec<Effect>,
}

impl<Effect> UpdateResult<Effect> {
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

    pub fn with_effect(effect: Effect) -> Self {
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
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }

    pub fn merge(mut self, other: Self) -> Self {
        self.dirty = self.dirty.merge(other.dirty);
        self.effects.extend(other.effects);
        self
    }
}

pub trait Component<Event, Message> {
    fn handle_event(&mut self, event: &Event) -> Option<Message>;
}

pub trait View<Model> {
    fn render(&self, model: &Model, frame: &mut Frame<'_>, area: Rect);
}

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
