// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(crate) const GIT_BRANCH_CONTEXT_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(1);

pub(crate) const PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL: std::time::Duration =
    std::time::Duration::from_mins(1);

pub(crate) const STATE_TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

pub(crate) const USAGE_ACCOUNT_REFRESH_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(30);

pub(crate) const RENDER_TICK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subscription {
    DaemonFrames,
    StdinBytes,
    WindowResize,
}

pub const ATTACH_CLIENT_SUBSCRIPTIONS: &[Subscription] = &[
    Subscription::DaemonFrames,
    Subscription::StdinBytes,
    Subscription::WindowResize,
];
