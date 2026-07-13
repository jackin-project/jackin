#![cfg(feature = "dhat-heap")]

use jackin_capsule::tui::components::pane::PaneBodyWidget;
use jackin_capsule::tui::socket_backend::SocketBackend;
use jackin_term::DamageGrid;
use ratatui::{Terminal, layout::Rect};
use std::sync::Mutex;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

static PROFILER_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn focused_full_snapshot_render_core_allocation_stays_bounded_after_warmup() {
    let _guard = PROFILER_LOCK
        .lock()
        .expect("profiler lock should not poison");
    let mut grid = DamageGrid::new(3, 20, 100);
    let backend = SocketBackend::new(20, 3);
    let mut terminal = Terminal::new(backend).expect("SocketBackend construction should not fail");
    let area = Rect::new(0, 0, 20, 3);
    let mut output = Vec::with_capacity(4096);

    grid.process(b"\x1b[1;1Hfirst row\x1b[2;1Hsecond row");
    {
        let snap = grid.dump();
        terminal
            .draw(|frame| frame.render_widget(PaneBodyWidget::new(&snap), area))
            .expect("SocketBackend draw should not fail");
    }
    terminal.backend_mut().drain_output_into(&mut output);
    output.clear();

    grid.process(b"\x1b[2;1Hchanged");
    let snap = grid.dump();

    let _profiler = dhat::Profiler::builder().testing().build();
    let before = dhat::HeapStats::get();

    terminal
        .draw(|frame| frame.render_widget(PaneBodyWidget::new(&snap), area))
        .expect("SocketBackend draw should not fail");
    drop(snap);
    terminal.backend_mut().drain_output_into(&mut output);
    std::hint::black_box(output.len());

    let after = dhat::HeapStats::get();
    let blocks = after.total_blocks - before.total_blocks;
    let bytes = after.total_bytes - before.total_bytes;
    let max_blocks = jackin_capsule::perf_budgets::FOCUSED_FULL_SNAPSHOT_MAX_BLOCKS as u64;
    let max_bytes = jackin_capsule::perf_budgets::FOCUSED_FULL_SNAPSHOT_MAX_BYTES as u64;
    dhat::assert!(
        blocks <= max_blocks,
        "expected only Ratatui Buffer::diff allocations"
    );
    dhat::assert!(
        bytes <= max_bytes,
        "expected only Ratatui Buffer::diff allocations"
    );
}

#[test]
fn focused_full_borrowed_view_render_core_allocation_stays_bounded_after_warmup() {
    let _guard = PROFILER_LOCK
        .lock()
        .expect("profiler lock should not poison");
    let mut grid = DamageGrid::new(3, 20, 100);
    let backend = SocketBackend::new(20, 3);
    let mut terminal = Terminal::new(backend).expect("SocketBackend construction should not fail");
    let area = Rect::new(0, 0, 20, 3);
    let mut output = Vec::with_capacity(4096);

    grid.process(b"\x1b[1;1Hfirst row\x1b[2;1Hsecond row");
    {
        let view = grid.scrollback_view(0, 3);
        terminal
            .draw(|frame| frame.render_widget(PaneBodyWidget::view(&view), area))
            .expect("SocketBackend draw should not fail");
    }
    terminal.backend_mut().drain_output_into(&mut output);
    output.clear();

    grid.process(b"\x1b[2;1Hchanged");

    let _profiler = dhat::Profiler::builder().testing().build();
    let before = dhat::HeapStats::get();

    {
        let view = grid.scrollback_view(0, 3);
        terminal
            .draw(|frame| frame.render_widget(PaneBodyWidget::view(&view), area))
            .expect("SocketBackend draw should not fail");
    }
    terminal.backend_mut().drain_output_into(&mut output);
    std::hint::black_box(output.len());

    let after = dhat::HeapStats::get();
    let blocks = after.total_blocks - before.total_blocks;
    let bytes = after.total_bytes - before.total_bytes;
    let max_blocks = jackin_capsule::perf_budgets::FOCUSED_BORROWED_VIEW_MAX_BLOCKS as u64;
    let max_bytes = jackin_capsule::perf_budgets::FOCUSED_BORROWED_VIEW_MAX_BYTES as u64;
    dhat::assert!(
        blocks <= max_blocks,
        "expected only Ratatui Buffer::diff allocations"
    );
    dhat::assert!(
        bytes <= max_bytes,
        "expected only Ratatui Buffer::diff allocations"
    );
}
