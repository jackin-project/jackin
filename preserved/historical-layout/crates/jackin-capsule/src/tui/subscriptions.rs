// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Long-lived TUI event sources for the capsule attach client.

/// One second is quick enough for operator-visible title/chrome updates after
/// `git checkout` while avoiding a 10Hz daemon wake-up just to inspect local
/// branch state.
pub(crate) const GIT_BRANCH_CONTEXT_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(1);

/// 60 s keeps the CI-status freshness within one PR turn while
/// staying well under `gh`'s default secondary-rate-limit budget.
/// The bar is operator-facing chrome, not a live feed.
pub(crate) const PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL: std::time::Duration =
    std::time::Duration::from_mins(1);

/// Per-second state tick for low-frequency timers such as feedback expiry
/// and conservative pull-request context refresh.
pub(crate) const STATE_TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Daemon-owned provider account refresh scheduler cadence. This does not call
/// providers every tick; it wakes the in-cache scheduler, which applies the
/// per-account ~5 minute + jitter due time and keeps renderers Turso-only.
pub(crate) const USAGE_ACCOUNT_REFRESH_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(30);

/// Render ticker: about 30 fps. Coalesces PTY-output bursts into one frame.
/// Cadence cap for event-driven composition: a burst coalesces to at most
/// one frame per this interval, while the first event after an idle gap
/// composes immediately (§3.10 of the capsule rendering plan).
pub(crate) const RENDER_TICK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

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
