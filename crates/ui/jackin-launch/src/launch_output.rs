// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::pin::Pin;

use jackin_core::LaunchOutputSink;

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
