#![cfg(feature = "dhat-heap")]

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_term::DamageGrid;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[test]
fn focused_process_dirty_patch_path_allocates_zero_after_warmup() {
    let mut grid = DamageGrid::new(24, 80, 1_000);
    grid.process(b"A");
    drop(grid.dump_dirty_patch());

    let _profiler = dhat::Profiler::builder().testing().build();
    let before = dhat::HeapStats::get();

    grid.process(b"B");
    let patch = grid.dump_dirty_patch();
    let changed_cells = patch
        .changed_spans()
        .map(|(_, _, cells)| cells.iter().filter(|cell| cell.has_contents()).count())
        .sum::<usize>();
    std::hint::black_box(changed_cells);

    let after = dhat::HeapStats::get();
    dhat::assert_eq!(after.total_blocks - before.total_blocks, 0);
    dhat::assert_eq!(after.total_bytes - before.total_bytes, 0);
}
