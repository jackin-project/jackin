// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn assert_command_names(
    cases: &[(&[&str], jackin_telemetry::schema::enums::CliCommandName)],
) {
    for (args, expected) in cases {
        let parsed = Cli::try_parse_from(std::iter::once("jackin").chain(args.iter().copied()))
            .unwrap_or_else(|error| panic!("failed to parse {args:?}: {error}"));
        let command = parsed.command.as_ref().expect("nested command");
        assert_eq!(command_name(command), *expected, "{args:?}");
    }
}

pub(super) fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip until 'm' (SGR) or other terminator
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

pub(super) fn help_text(args: &[&str]) -> String {
    let err = Cli::try_parse_from(args).unwrap_err();
    strip_ansi(&err.to_string())
}
