// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Path text helpers shared by non-TUI and TUI crates.

/// Shorten an absolute path by replacing the operator's `$HOME` prefix with `~`.
#[must_use]
pub fn shorten_home(path: &str) -> String {
    let Some(home) = std::env::var_os("HOME") else {
        return path.to_owned();
    };
    let home = home.to_string_lossy().into_owned();
    if home.is_empty() {
        return path.to_owned();
    }
    // Only collapse when the next character after `$HOME` is a path separator
    // (or end of string). Otherwise `/Users/alice.notmine` would incorrectly
    // compact to `~.notmine`.
    match path.strip_prefix(&home) {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
        _ => path.to_owned(),
    }
}

#[cfg(test)]
mod tests;
