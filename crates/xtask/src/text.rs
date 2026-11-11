// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;

pub(crate) fn plain(value: impl Display) -> String {
    let text = value.to_string();
    let mut out = String::with_capacity(text.len());
    let mut run = 0;
    for (index, c) in text.char_indices() {
        if matches!(c, '"' | '\'' | '\\') {
            out.extend(text[run..index].escape_debug());
            out.push(c);
            run = index + c.len_utf8();
        }
    }
    out.extend(text[run..].escape_debug());
    out
}
