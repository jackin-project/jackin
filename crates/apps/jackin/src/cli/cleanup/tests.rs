// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::cli::Cli;
use clap::Parser;

fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            for inner in chars.by_ref() {
                if inner.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

fn help_text(args: &[&str]) -> String {
    let err = Cli::try_parse_from(args).unwrap_err();
    strip_ansi(&err.to_string())
}

#[test]
fn eject_help_shows_examples() {
    let help = help_text(&["jackin", "eject", "--help"]);
    assert!(help.contains("Stop a role and clean up its container"));
    assert!(help.contains("jackin eject agent-smith --all"));
    assert!(help.contains("jackin eject agent-smith --purge"));
}

#[test]
fn purge_help_shows_examples() {
    let help = help_text(&["jackin", "purge", "--help"]);
    assert!(help.contains("Delete persisted state"));
    assert!(help.contains("jackin purge agent-smith --all"));
}
