// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(any(target_os = "macos", target_os = "linux"))]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail the test by panicking"
)]

mod app;
mod baseline;
mod capsule;
mod coverage;
mod fixed;
mod manager;
mod steps;
