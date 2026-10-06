// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `LaunchTuiOutputSink`: adapter from the `LaunchOutputSink` port (in
//! jackin-core) to this crate's product-owned output and animation helpers.
//!
//! `jackin-runtime` obtains the singleton via `progress::launch_output()`
//! (self-owned static, exactly as it owns `host_terminal()`). This keeps
//! the production call sites in runtime free of presentation details.

use std::future::Future;
use std::pin::Pin;

use jackin_core::LaunchOutputSink;

/// Zero-sized adapter; forwards the four launch output side-effects to the
/// presentation crate's helpers (`print_deploying`, `step_fail`, `warp_*`).
#[derive(Debug, Clone, Copy, Default)]
pub struct LaunchTuiOutputSink;

impl LaunchOutputSink for LaunchTuiOutputSink {
    fn print_deploying<'a>(&'a self, role_name: &'a str) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
        Box::pin(crate::output::print_deploying(role_name))
    }

    fn step_fail(&self, msg: &str) {
        crate::output::step_fail(msg);
    }

    fn warp_out(&self, host_screen_owned: bool) {
        crate::animation::warp_out(host_screen_owned);
    }

    fn warp_end_caption(&self, elapsed: Option<std::time::Duration>, host_screen_owned: bool) {
        crate::animation::warp_end_caption(elapsed, host_screen_owned);
    }
}
