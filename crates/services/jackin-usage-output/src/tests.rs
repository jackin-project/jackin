// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn writers_emit_without_panic() {
    stdout_line(format_args!("line"));
    stdout_empty_line();
    stdout_fragment(format_args!("fragment"));
    stderr_line(format_args!("error line"));
}
