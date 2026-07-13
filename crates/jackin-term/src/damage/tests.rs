// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{DirtySpans, DirtyTracker};

#[test]
fn dirty_rows_are_sorted_and_deduplicated() {
    let mut dirty = DirtyTracker::new(10);

    dirty.mark_range(3, 1, 2);
    dirty.mark_range(1, 4, 7);
    dirty.mark_range(3, 0, 4);
    dirty.mark_range(2, 9, 10);

    let DirtySpans::Rows(rows) = dirty.take() else {
        panic!("expected row-specific dirty spans");
    };
    assert_eq!(
        rows.iter()
            .map(|span| (span.row, span.start_col, span.end_col))
            .collect::<Vec<_>>(),
        [(1, 4, 7), (2, 9, 10), (3, 0, 4)]
    );
    assert!(!dirty.is_dirty());
}

#[test]
fn dirty_row_marks_full_span() {
    let mut dirty = DirtyTracker::new(10);

    dirty.mark_range(2, 4, 5);
    dirty.mark_row(2);

    let DirtySpans::Rows(rows) = dirty.take() else {
        panic!("expected row-specific dirty spans");
    };
    assert_eq!(
        rows.iter()
            .map(|span| (span.row, span.start_col, span.end_col))
            .collect::<Vec<_>>(),
        [(2, 0, u16::MAX)]
    );
}
