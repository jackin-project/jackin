// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::mount_diff::{MountDiff, MountDiffItem, classify_mount_diffs};

#[derive(Debug, PartialEq, Eq)]
struct FixtureMount {
    dst: String,
    src: String,
}

impl MountDiffItem for FixtureMount {
    fn dst(&self) -> &str {
        &self.dst
    }
}

fn mount(dst: &str, src: &str) -> FixtureMount {
    FixtureMount {
        dst: dst.to_owned(),
        src: src.to_owned(),
    }
}

#[test]
fn identical_mounts_classify_as_unchanged() {
    let original = [mount("/data", "src-a")];
    let pending = [mount("/data", "src-a")];
    assert_eq!(
        classify_mount_diffs(&original, &pending),
        [MountDiff::Unchanged(&pending[0])]
    );
}

#[test]
fn changed_source_classifies_as_modified_with_both_sides() {
    let original = [mount("/data", "src-a")];
    let pending = [mount("/data", "src-b")];
    assert_eq!(
        classify_mount_diffs(&original, &pending),
        [MountDiff::Modified {
            original: &original[0],
            pending: &pending[0],
        }]
    );
}

#[test]
fn missing_and_new_destinations_classify_as_removed_and_added() {
    let original = [mount("/old", "src-a")];
    let pending = [mount("/new", "src-b")];
    assert_eq!(
        classify_mount_diffs(&original, &pending),
        [
            MountDiff::Added(&pending[0]),
            MountDiff::Removed(&original[0]),
        ]
    );
}

#[test]
fn empty_original_marks_everything_added() {
    let pending = [mount("/a", "src-a"), mount("/b", "src-b")];
    assert_eq!(
        classify_mount_diffs(&[], &pending),
        [MountDiff::Added(&pending[0]), MountDiff::Added(&pending[1]),]
    );
}
