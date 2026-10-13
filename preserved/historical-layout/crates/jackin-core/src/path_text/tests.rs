// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::shorten_home;

#[test]
fn shorten_home_returns_path_when_no_match() {
    let home = std::env::var("HOME").unwrap_or_default();
    let alien = if home == "/" {
        "etc/hosts".to_owned()
    } else {
        format!("{home}.notmine")
    };
    assert_eq!(shorten_home(&alien), alien);
}
