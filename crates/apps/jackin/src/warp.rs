// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub fn warp_intro(host_screen_owned: bool) {
    jackin_launch::animation::warp_intro(host_screen_owned);
}

pub fn warp_out(host_screen_owned: bool) {
    jackin_launch::animation::warp_out(host_screen_owned);
}

pub fn warp_end_caption(elapsed: Option<std::time::Duration>, host_screen_owned: bool) {
    jackin_launch::animation::warp_end_caption(elapsed, host_screen_owned);
}
