// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod clipboard;
mod lease;
mod open;
mod replies;
mod restart;
mod shell;
mod support;

#[expect(dead_code, reason = "each test binary uses part of the shared tap")]
#[path = "../common/tap.rs"]
mod tap;
