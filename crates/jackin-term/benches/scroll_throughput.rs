// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Scroll-throughput benchmark for the `scroll_up` line-feed hot path.
//!
//! `scroll_up` runs on every newline once the screen is full — the most
//! frequent terminal operation under normal output. This measures processing a
//! large run of scrolling lines, on both the primary screen (scrollback active)
//! and the alternate screen (no scrollback), so a ring-rotation vs clone-shift
//! change can be compared against a recorded baseline.
//!
//! ```sh
//! cargo bench -p jackin-term --bench scroll_throughput
//! ```

use criterion::Criterion;
use jackin_term::DamageGrid;
use std::hint::black_box;

const ROWS: u16 = 40;
const COLS: u16 = 120;
const SCROLLBACK: usize = 10_000;
const LINES: usize = 5_000;

fn line_run() -> Vec<u8> {
    let mut out = Vec::new();
    for i in 0..LINES {
        out.extend_from_slice(format!("line {i:05} ").as_bytes());
        for col in 0..(COLS as usize / 2) {
            out.push(b'a' + (col % 26) as u8);
        }
        out.extend_from_slice(b"\r\n");
    }
    out
}

fn bench_scroll(c: &mut Criterion) {
    let mut group = c.benchmark_group("scroll_throughput");
    group.sample_size(20);

    let primary = {
        let mut s = b"\x1b[2J\x1b[H".to_vec();
        s.extend_from_slice(&line_run());
        s
    };
    group.bench_function("primary_5000_lines_with_scrollback", |b| {
        b.iter(|| {
            let mut grid = DamageGrid::new(ROWS, COLS, SCROLLBACK);
            grid.process(black_box(&primary));
            black_box(grid.dump().cursor);
        });
    });

    let alternate = {
        // Enter the alternate screen first: scroll there never touches scrollback.
        let mut s = b"\x1b[?1049h\x1b[2J\x1b[H".to_vec();
        s.extend_from_slice(&line_run());
        s
    };
    group.bench_function("alternate_5000_lines_no_scrollback", |b| {
        b.iter(|| {
            let mut grid = DamageGrid::new(ROWS, COLS, SCROLLBACK);
            grid.process(black_box(&alternate));
            black_box(grid.dump().cursor);
        });
    });

    group.finish();
}

fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    bench_scroll(&mut criterion);
    criterion.final_summary();
}
