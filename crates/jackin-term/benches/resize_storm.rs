//! Resize-storm benchmark for `DamageGrid::set_size`.
//!
//! Interactive window drags deliver a burst of resize events in quick
//! succession. Before the in-place rewrite, every `set_size` call — even a
//! same-size no-op — rebuilt both the primary and alternate screens from
//! scratch: two full grid allocations, an arena scan per row, and a deep
//! clone of every retained cell. This measures four representative resize
//! shapes so a before/after comparison can show the in-place win (or catch a
//! regression).
//!
//! Note on the fixture: `set_size` never touches `self.scrollback` (there is
//! no reflow-on-resize), so a scrollback-heavy fixture does not stress the
//! resize path itself. The grid is preloaded with content in the *visible*
//! rows (via `grid.process`) so the retained-cell copy/clone work in each
//! scenario is representative; a realistic amount of scrollback is also
//! populated so the fixture matches an in-use session, even though resize
//! does not read it.
//!
//! ```sh
//! cargo bench --bench resize_storm -p jackin-term -- --quick
//! ```

use criterion::{BatchSize, Criterion};
use jackin_term::DamageGrid;
use std::hint::black_box;

const ROWS: u16 = 40;
const COLS: u16 = 120;
const SCROLLBACK: usize = 10_000;
const SCROLLBACK_LINES: usize = 2_000;

/// A grid with every visible cell populated and a realistic amount of
/// scrollback behind it (resize does not read the scrollback, but a fixture
/// with none would understate a real session's retained-row size).
fn preloaded_grid() -> DamageGrid {
    let mut grid = DamageGrid::new(ROWS, COLS, SCROLLBACK);
    let mut burst = Vec::new();
    for i in 0..SCROLLBACK_LINES {
        burst.extend_from_slice(format!("line {i:05} ").as_bytes());
        for col in 0..(COLS as usize / 2) {
            burst.push(b'a' + (col % 26) as u8);
        }
        burst.extend_from_slice(b"\r\n");
    }
    grid.process(&burst);
    grid
}

fn bench_resize(c: &mut Criterion) {
    let mut group = c.benchmark_group("resize_storm");
    group.sample_size(20);

    // (a) width + height resize: grow both dimensions.
    group.bench_function("width_and_height_resize", |b| {
        b.iter_batched(
            preloaded_grid,
            |mut grid| {
                grid.set_size(ROWS + 10, COLS + 20);
                black_box(grid.dump().cursor);
            },
            BatchSize::SmallInput,
        );
    });

    // (b) height-only resize: cols unchanged, rows grow.
    group.bench_function("height_only_resize", |b| {
        b.iter_batched(
            preloaded_grid,
            |mut grid| {
                grid.set_size(ROWS + 10, COLS);
                black_box(grid.dump().cursor);
            },
            BatchSize::SmallInput,
        );
    });

    // (c) same-size no-op resize.
    group.bench_function("same_size_resize", |b| {
        b.iter_batched(
            preloaded_grid,
            |mut grid| {
                grid.set_size(ROWS, COLS);
                black_box(grid.dump().cursor);
            },
            BatchSize::SmallInput,
        );
    });

    // (d) a storm of 20 alternating resizes, as an interactive window drag delivers.
    group.bench_function("alternating_resize_storm", |b| {
        b.iter_batched(
            preloaded_grid,
            |mut grid| {
                for i in 0..20u16 {
                    if i % 2 == 0 {
                        grid.set_size(ROWS + 10, COLS + 20);
                    } else {
                        grid.set_size(ROWS, COLS);
                    }
                }
                black_box(grid.dump().cursor);
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    bench_resize(&mut criterion);
    criterion.final_summary();
}
