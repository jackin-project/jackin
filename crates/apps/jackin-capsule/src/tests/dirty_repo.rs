// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::exit_assess::DirtyRepo;
use jackin_core::ChangedFile;

fn repo(path: &str, changed: usize, unpushed: usize) -> DirtyRepo {
    DirtyRepo {
        path: path.to_owned(),
        changed: (0..changed)
            .map(|index| ChangedFile {
                status: 'M',
                path: format!("file{index}.rs"),
            })
            .collect(),
        unpushed,
    }
}

#[test]
fn label_uses_final_path_segment() {
    assert_eq!(repo("/srv/repos/payments", 0, 0).label(), "payments");
    assert_eq!(repo("/srv/repos/payments/", 0, 0).label(), "payments");
    assert_eq!(repo("standalone", 0, 0).label(), "standalone");
}

#[test]
fn summary_line_lists_change_and_unpushed_counts() {
    assert_eq!(
        repo("repos/payments", 2, 1).summary_line(),
        "payments   2 changed · 1 unpushed"
    );
}

#[test]
fn summary_line_without_activity_shows_label_only() {
    assert_eq!(repo("repos/payments", 0, 0).summary_line(), "payments   ");
}
