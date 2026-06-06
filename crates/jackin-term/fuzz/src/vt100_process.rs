//! Fuzz target: feed arbitrary bytes to jackin-term and the vt100 oracle.
//!
//! Phase 1 of Defect 45. The goal: **zero panics**, ever, on any byte sequence.
//! Targets the panic class identified in doy/vt100-rust PR #30 (wide-char truncation).
//!
//! Run locally (short budget, CI-suitable):
//!   cargo fuzz run vt100_process -- -max_total_time=60
//! Run overnight (nightly, deep coverage):
//!   cargo fuzz run vt100_process -- -max_total_time=86400

#![no_main]
use jackin_term::{Cell, Color, DamageGrid};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Standard screen size. Vary via structured input if a size-dependent panic is found.
    let mut grid = DamageGrid::new(24, 80, 10_000);
    let mut parser = vt100::Parser::new(24, 80, 10_000);
    grid.process(data);
    parser.process(data);

    // vt100 and vte deliberately diverge on malformed UTF-8, some Unicode
    // width/combining behavior, obscure C0 controls, and historical ESC
    // charset selectors. Keep arbitrary bytes for panic coverage; use the
    // deterministic corpus tests for escape-sequence equivalence, and assert
    // fuzz equivalence for plain ASCII/control streams.
    if !is_oracle_aligned_ascii(data) || data.contains(&b'\x1b') {
        return;
    }

    let (grid_rows, grid_cols) = grid.size();
    let screen = parser.screen();
    let (oracle_rows, oracle_cols) = screen.size();
    assert_eq!((grid_rows, grid_cols), (oracle_rows, oracle_cols));
    assert_eq!(grid.cursor_position(), screen.cursor_position());
    assert_eq!(grid.alternate_screen(), screen.alternate_screen());

    for r in 0..grid_rows {
        for c in 0..grid_cols {
            let left = grid.cell(r, c).expect("grid cell in bounds");
            let right = screen.cell(r, c).expect("oracle cell in bounds");
            assert_eq!(left.contents(), right.contents());
            assert_eq!(left.is_wide, right.is_wide());
            assert_eq!(left.is_wide_continuation, right.is_wide_continuation());
            assert_eq!(color(left.fgcolor()), oracle_color(right.fgcolor()));
            assert_eq!(color(left.bgcolor()), oracle_color(right.bgcolor()));
            assert_eq!(attrs(left), oracle_attrs(right));
        }
    }
});

fn attrs(cell: &Cell) -> (bool, bool, bool, bool) {
    (cell.bold(), cell.italic(), cell.underline(), cell.inverse())
}

fn oracle_attrs(cell: &vt100::Cell) -> (bool, bool, bool, bool) {
    (cell.bold(), cell.italic(), cell.underline(), cell.inverse())
}

fn color(color: Color) -> (u8, u8, u8, u8) {
    match color {
        Color::Default => (0, 0, 0, 0),
        Color::Idx(idx) => (1, idx, 0, 0),
        Color::Rgb(red, green, blue) => (2, red, green, blue),
    }
}

fn oracle_color(color: vt100::Color) -> (u8, u8, u8, u8) {
    match color {
        vt100::Color::Default => (0, 0, 0, 0),
        vt100::Color::Idx(idx) => (1, idx, 0, 0),
        vt100::Color::Rgb(red, green, blue) => (2, red, green, blue),
    }
}

fn is_oracle_aligned_ascii(data: &[u8]) -> bool {
    data.iter()
        .copied()
        .all(|byte| matches!(byte, b'\x08' | b'\t' | b'\n' | b'\r' | b'\x1b' | b' '..=b'~'))
}
