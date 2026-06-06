#![cfg(feature = "dhat-heap")]

use jackin_term::DamageGrid;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[test]
fn focused_process_dirty_spans_path_allocates_zero_after_warmup() {
    let mut grid = DamageGrid::new(24, 80, 1_000);
    grid.process(b"A");
    drop(grid.dirty_spans());

    let _profiler = dhat::Profiler::builder().testing().build();
    let before = dhat::HeapStats::get();

    grid.process(b"B");
    let spans = grid.dirty_spans();
    std::hint::black_box(spans);

    let after = dhat::HeapStats::get();
    dhat::assert_eq!(after.total_blocks - before.total_blocks, 0);
    dhat::assert_eq!(after.total_bytes - before.total_bytes, 0);
}
