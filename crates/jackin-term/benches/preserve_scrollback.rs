//! `preserve_visible_rows_to_scrollback` genuine miss path (plan 026).
//!
//! Drive ED2 after content mutation so the preserve path runs (not the
//! unchanged/dedupe short-circuit).
//!
//! ```sh
//! cargo bench -p jackin-term --bench preserve_scrollback -- --test
//! ```

use std::hint::black_box;

use criterion::Criterion;
use jackin_term::DamageGrid;

const ROWS: u16 = 40;
const COLS: u16 = 120;
const SCROLLBACK: usize = 5_000;

fn fill_screen() -> Vec<u8> {
    let mut out = b"\x1b[2J\x1b[H".to_vec();
    for r in 0..ROWS {
        out.extend_from_slice(format!("row {r:03} ").as_bytes());
        for c in 0..(COLS as usize / 2) {
            out.push(b'a' + ((r as usize + c) % 26) as u8);
        }
        out.extend_from_slice(b"\r\n");
    }
    out
}

fn bench_preserve(c: &mut Criterion) {
    let mut group = c.benchmark_group("preserve_scrollback");
    group.sample_size(30);
    let fill = fill_screen();
    // Content + ED2: preserve-on-clear miss path (mutated since preserve).
    let mut sequence = fill.clone();
    sequence.extend_from_slice(b"\x1b[2J");

    group.bench_function("ed2_preserve_after_fill", |b| {
        b.iter(|| {
            let mut grid = DamageGrid::new(ROWS, COLS, SCROLLBACK);
            grid.process(black_box(&sequence));
            black_box(grid.dump().cursor);
        });
    });

    // Baseline: second ED2 without mutation should skip preserve (dedupe path).
    let mut double_clear = sequence.clone();
    double_clear.extend_from_slice(b"\x1b[2J");
    group.bench_function("ed2_dedupe_second_clear", |b| {
        b.iter(|| {
            let mut grid = DamageGrid::new(ROWS, COLS, SCROLLBACK);
            grid.process(black_box(&double_clear));
            black_box(grid.dump().cursor);
        });
    });

    group.finish();
}

fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    bench_preserve(&mut criterion);
    criterion.final_summary();
}
