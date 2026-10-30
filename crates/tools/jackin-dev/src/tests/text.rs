// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;

use crate::{shell_quote, yes_no};

#[test]
fn plain_paths_pass_through_unquoted() {
    assert_eq!(
        shell_quote(OsStr::new("abc-1.2_3/x:y+z")),
        "abc-1.2_3/x:y+z"
    );
}

#[test]
fn paths_with_spaces_are_single_quoted() {
    assert_eq!(shell_quote(OsStr::new("a b")), "'a b'");
}

#[test]
fn embedded_quotes_escape_without_splitting_words() {
    assert_eq!(shell_quote(OsStr::new("it's")), "'it'\"'\"'s'");
}

#[test]
fn yes_no_spells_booleans() {
    assert_eq!(yes_no(true), "yes");
    assert_eq!(yes_no(false), "no");
}
