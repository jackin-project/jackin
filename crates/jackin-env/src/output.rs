// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Plain stderr writer for operator-facing environment warnings.

use std::fmt::Arguments;
use std::io::Write as _;

pub(crate) fn stderr_line(args: Arguments<'_>) {
    let mut stderr = std::io::stderr().lock();
    drop(writeln!(stderr, "{args}"));
}
