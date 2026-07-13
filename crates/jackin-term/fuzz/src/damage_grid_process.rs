// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Fuzz target: feed arbitrary bytes to jackin-term.
//!
//! Phase 1 of Defect 45. The goal: **zero panics**, ever, on any byte sequence.
//!
//! Run locally (short budget, CI-suitable):
//!   cargo fuzz run --sanitizer none damage_grid_process -- -max_total_time=60
//! Run overnight (deep coverage):
//!   cargo fuzz run --sanitizer none damage_grid_process -- -max_total_time=86400

#![no_main]
use jackin_term::{Cell, Color, DamageGrid};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut one_shot = DamageGrid::new(24, 80, 10_000);
    let mut split = DamageGrid::new(24, 80, 10_000);
    one_shot.process(data);
    for byte in data {
        split.process(std::slice::from_ref(byte));
    }

    assert_eq!(one_shot.size(), split.size());
    assert_eq!(one_shot.cursor_position(), split.cursor_position());
    assert_eq!(one_shot.alternate_screen(), split.alternate_screen());
    assert_eq!(one_shot.hide_cursor(), split.hide_cursor());
    assert_eq!(one_shot.application_cursor(), split.application_cursor());

    let (rows, cols) = one_shot.size();
    let (cursor_row, cursor_col) = one_shot.cursor_position();
    assert!(cursor_row < rows);
    assert!(cursor_col <= cols);

    for r in 0..rows {
        for c in 0..cols {
            let left = one_shot.cell(r, c).expect("grid cell in bounds");
            let right = split.cell(r, c).expect("split grid cell in bounds");
            assert_eq!(left.contents(), right.contents());
            assert_eq!(left.is_wide, right.is_wide);
            assert_eq!(left.is_wide_continuation, right.is_wide_continuation);
            assert_eq!(color(left.fgcolor()), color(right.fgcolor()));
            assert_eq!(color(left.bgcolor()), color(right.bgcolor()));
            assert_eq!(attrs(left), attrs(right));
        }
    }
});

fn attrs(cell: &Cell) -> (bool, bool, bool, bool) {
    (cell.bold(), cell.italic(), cell.underline(), cell.inverse())
}

fn color(color: Color) -> (u8, u8, u8, u8) {
    match color {
        Color::Default => (0, 0, 0, 0),
        Color::Idx(idx) => (1, idx, 0, 0),
        Color::Rgb(red, green, blue) => (2, red, green, blue),
    }
}
