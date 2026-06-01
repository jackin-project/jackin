//! Long-lived event sources that drive the host console TUI loop.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subscription {
    /// Keyboard, mouse, resize, focus, and paste events from crossterm.
    TerminalEvents,
    /// Periodic redraw tick for spinners and animated picker surfaces.
    AnimationTick,
}

pub const CONSOLE_SUBSCRIPTIONS: &[Subscription] =
    &[Subscription::TerminalEvents, Subscription::AnimationTick];
