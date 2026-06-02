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

pub fn drift_check_worker_disconnected_message() -> &'static str {
    "drift check worker disconnected"
}

pub fn isolation_cleanup_worker_disconnected_message() -> &'static str {
    "isolation cleanup worker disconnected"
}

pub fn role_loader_worker_disconnected_message() -> &'static str {
    "role loader worker disconnected"
}

pub fn op_read_worker_disconnected_message() -> &'static str {
    "op read worker disconnected"
}

pub fn instance_refresh_worker_disconnected_message() -> &'static str {
    "instance refresh worker disconnected"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_disconnect_messages_are_subscription_owned() {
        assert_eq!(
            drift_check_worker_disconnected_message(),
            "drift check worker disconnected"
        );
        assert_eq!(
            isolation_cleanup_worker_disconnected_message(),
            "isolation cleanup worker disconnected"
        );
        assert_eq!(
            role_loader_worker_disconnected_message(),
            "role loader worker disconnected"
        );
        assert_eq!(
            op_read_worker_disconnected_message(),
            "op read worker disconnected"
        );
        assert_eq!(
            instance_refresh_worker_disconnected_message(),
            "instance refresh worker disconnected"
        );
    }
}
