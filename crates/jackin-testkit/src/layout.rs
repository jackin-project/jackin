// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage::view::text::width;
use uuid::Uuid;

const SUBCOMMANDS: [&str; 11] = [
    "account",
    "usage",
    "reconnect",
    "rm",
    "ls",
    "redeem",
    "details",
    "refresh",
    "connect",
    "providers",
    "snapshot",
];

const GLUED: [char; 4] = [')', '.', ',', ';'];

fn is_argument(word: &str) -> bool {
    SUBCOMMANDS.contains(&word)
        || word.starts_with("--")
        || (word.starts_with('<') && word.ends_with('>'))
        || Uuid::parse_str(word).is_ok()
}

#[must_use]
pub fn is_unsplittable_command(line: &str, columns: usize) -> bool {
    let line = line.trim_end();
    let indent = width(line) - width(line.trim_start());
    let command = line.trim_start().trim_end_matches(GLUED);
    let mut words = command.split(' ');
    words.next() == Some("jackin") && words.all(is_argument) && indent + width(command) > columns
}
