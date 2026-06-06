//! Fuzz target: feed arbitrary bytes to vt100::Parser and assert no panic.
//!
//! Phase 1 of Defect 45. The goal: **zero panics**, ever, on any byte sequence.
//! Targets the panic class identified in doy/vt100-rust PR #30 (wide-char truncation).
//!
//! When Phase 2 lands jackin_term::DamageGrid, add a second `Parser::process(data)`
//! call with the jackin-term grid and assert the two grids are identical — this
//! fuzz target becomes the live differential fuzzer.
//!
//! Run locally (short budget, CI-suitable):
//!   cargo fuzz run vt100_process -- -max_total_time=60
//! Run overnight (nightly, deep coverage):
//!   cargo fuzz run vt100_process -- -max_total_time=86400

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Standard screen size. Vary via structured input if a size-dependent panic is found.
    let mut parser = vt100::Parser::new(24, 80, 10_000);
    parser.process(data);

    // Snapshot the screen: any panic during cell access is a bug.
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    for r in 0..rows {
        for c in 0..cols {
            drop(screen.cell(r, c));
        }
    }
});
