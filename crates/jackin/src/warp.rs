// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch-intro / outro / closing-screen rituals, lifted from the
//! `crate::tui` shim. The binary previously re-exported these as
//! `crate::warp::warp_intro` etc.; the B2 shim removal relocates them
//! here so callers can keep their existing call shape while the shim
//! itself goes away.

/// Entry ritual — opening phrases then a hyperspace jump into the Construct.
///
/// `host_screen_owned` should be `jackin_tui::ownership::host_screen_owned()`.
pub fn warp_intro(host_screen_owned: bool) {
    jackin_launch_tui::animation::warp_intro(host_screen_owned);
}

/// Exit ritual — drop out of hyperspace.
///
/// `host_screen_owned` should be `jackin_tui::ownership::host_screen_owned()`.
pub fn warp_out(host_screen_owned: bool) {
    jackin_launch_tui::animation::warp_out(host_screen_owned);
}

/// Closing screen shown when the last container leaves.
///
/// `host_screen_owned` should be `jackin_tui::ownership::host_screen_owned()`.
pub fn warp_end_caption(elapsed: Option<std::time::Duration>, host_screen_owned: bool) {
    jackin_launch_tui::animation::warp_end_caption(elapsed, host_screen_owned);
}
