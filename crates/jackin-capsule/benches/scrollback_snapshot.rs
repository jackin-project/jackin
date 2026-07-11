//! Scrollback content-snapshot benchmark.
//!
//! `pane_content_from_damagegrid` materializes retained scrollback + live
//! screen into owned `RowSnapshot`s. Link-hover resolution only needs a
//! handful of rows; the range-scoped API bounds the allocation. This bench
//! compares full-range vs narrow-range on a deep scrollback grid so plan 026's
//! win is measurable.
//!
//! ```sh
//! cargo bench --bench scrollback_snapshot -p jackin-capsule -- --quick
//! ```

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use jackin_capsule::tui::pane_snapshot::{
    pane_content_from_damagegrid, pane_content_range_from_damagegrid,
};
use jackin_term::DamageGrid;

const SCREEN_ROWS: u16 = 40;
const SCREEN_COLS: u16 = 120;
/// Near the 10k retained-scrollback bound used by the capsule.
const SCROLLBACK_ROWS: usize = 8_000;
const VIEWPORT_COLS: u16 = SCREEN_COLS;
/// Hover/click window: single content row (see `mouse_input` resolver).
const NARROW_WINDOW: usize = 1;

fn make_deep_scrollback_grid() -> DamageGrid {
    let mut grid = DamageGrid::new(
        SCREEN_ROWS,
        SCREEN_COLS,
        SCROLLBACK_ROWS.saturating_add(usize::from(SCREEN_ROWS)),
    );
    let total_lines = SCROLLBACK_ROWS.saturating_add(usize::from(SCREEN_ROWS));
    for i in 0..total_lines {
        let line = format!("{i:05}: The quick brown fox jumps over the lazy dog on line {i}.\r\n");
        grid.process(line.as_bytes());
    }
    grid
}

fn bench_scrollback_snapshot(c: &mut Criterion) {
    let grid = make_deep_scrollback_grid();
    let filled = grid.scrollback_len();
    let total = filled.saturating_add(usize::from(SCREEN_ROWS));
    // Anchor near the middle of scrollback — worst-case full materialization
    // vs a 1-row window around a hover target.
    let anchor = filled / 2;
    let narrow = anchor..anchor.saturating_add(NARROW_WINDOW);

    let mut group = c.benchmark_group("scrollback_snapshot");
    group.sample_size(20);
    group.throughput(Throughput::Elements(total as u64));

    group.bench_function(BenchmarkId::new("full_range", total), |b| {
        b.iter(|| {
            let snap = pane_content_from_damagegrid(black_box(&grid), VIEWPORT_COLS);
            black_box(snap.len());
        });
    });

    group.throughput(Throughput::Elements(NARROW_WINDOW as u64));
    group.bench_function(BenchmarkId::new("narrow_range", NARROW_WINDOW), |b| {
        b.iter(|| {
            let snap = pane_content_range_from_damagegrid(
                black_box(&grid),
                VIEWPORT_COLS,
                black_box(narrow.clone()),
            );
            black_box(snap.len());
        });
    });

    group.finish();
}

criterion_group!(benches, bench_scrollback_snapshot);
criterion_main!(benches);
