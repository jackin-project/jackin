//! Focused-pane present-frame benchmark for Defect 52.
//!
//! Run timing:
//! ```sh
//! cargo bench -p jackin-term --bench present_frame
//! ```
//!
//! Run heap profiling:
//! ```sh
//! cargo bench -p jackin-term --bench present_frame --features dhat-heap
//! ```

use criterion::{BatchSize, Criterion};
use jackin_term::{DamageGrid, DirtySpans};
use std::hint::black_box;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const ROWS: u16 = 40;
const COLS: u16 = 120;
const SCROLLBACK: usize = 10_000;

fn seed_stream() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"\x1b[2J\x1b[H");
    for row in 0..ROWS {
        out.extend_from_slice(format!("\x1b[{};1H", row + 1).as_bytes());
        for col in 0..COLS {
            let ch = b'a' + ((row + col) % 26) as u8;
            out.push(ch);
        }
    }
    out
}

fn update_stream(frame: u16) -> Vec<u8> {
    let row = (frame % ROWS) + 1;
    format!("\x1b[{row};1Hframe={frame:04} status=running\x1b[K").into_bytes()
}

fn seeded_damage_grid() -> DamageGrid {
    let mut grid = DamageGrid::new(ROWS, COLS, SCROLLBACK);
    grid.process(&seed_stream());
    drop(grid.dirty_spans());
    grid
}

fn dirty_count(spans: DirtySpans) -> usize {
    match spans {
        DirtySpans::All => ROWS as usize,
        DirtySpans::Rows(rows) => rows.len(),
    }
}

fn bench_present_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("present_frame");
    group.sample_size(20);

    group.bench_function("jackin_term_process_update_and_dump", |b| {
        b.iter_batched(
            seeded_damage_grid,
            |mut grid| {
                grid.process(black_box(&update_stream(7)));
                let snapshot = grid.dump();
                black_box((snapshot.rows, snapshot.cols, snapshot.cursor));
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("jackin_term_process_update_and_dirty_spans", |b| {
        b.iter_batched(
            seeded_damage_grid,
            |mut grid| {
                grid.process(black_box(&update_stream(11)));
                black_box(dirty_count(grid.dirty_spans()));
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("jackin_term_process_update_and_text_dump", |b| {
        b.iter_batched(
            seeded_damage_grid,
            |mut grid| {
                grid.process(black_box(&update_stream(13)));
                black_box(grid.dump().to_text());
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    let mut criterion = Criterion::default().configure_from_args();
    bench_present_frame(&mut criterion);
    criterion.final_summary();
}
