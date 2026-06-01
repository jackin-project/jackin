//! Long-lived TUI event sources for the capsule attach client.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subscription {
    /// Binary attach frames arriving from the multiplexer daemon.
    DaemonFrames,
    /// Bytes read from the operator's stdin.
    StdinBytes,
    /// SIGWINCH notifications from the outer terminal.
    WindowResize,
}

pub const ATTACH_CLIENT_SUBSCRIPTIONS: &[Subscription] = &[
    Subscription::DaemonFrames,
    Subscription::StdinBytes,
    Subscription::WindowResize,
];
