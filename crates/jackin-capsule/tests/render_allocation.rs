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
    dhat::assert!(
        blocks <= 3,
        "expected only Ratatui Buffer::diff allocations"
    );
    dhat::assert!(
        bytes <= 1024,
        "expected only Ratatui Buffer::diff allocations"
    );
}

#[test]
fn focused_dirty_patch_direct_encoder_allocates_zero_after_warmup() {
    let _guard = PROFILER_LOCK
        .lock()
        .expect("profiler lock should not poison");
    let mut grid = DamageGrid::new(3, 20, 100);
    let mut backend = SocketBackend::new(20, 3);
    let area = Rect::new(0, 0, 20, 3);
    let mut output = Vec::with_capacity(4096);

    grid.process(b"\x1b[1;1Hfirst row\x1b[2;1Hsecond row");
    {
        let patch = grid.dump_dirty_patch();
        backend.draw_grid_patch(area, &patch);
    }
    backend.drain_output_into(&mut output);
    output.clear();

    let _profiler = dhat::Profiler::builder().testing().build();
    let before = dhat::HeapStats::get();

    grid.process(b"\x1b[2;1Hchanged");
    {
        let patch = grid.dump_dirty_patch();
        backend.draw_grid_patch(area, &patch);
    }
    backend.drain_output_into(&mut output);
    std::hint::black_box(output.len());

    let after = dhat::HeapStats::get();
    dhat::assert_eq!(after.total_blocks - before.total_blocks, 0);
    dhat::assert_eq!(after.total_bytes - before.total_bytes, 0);
}

#[test]
fn focused_dirty_patch_allocates_zero_after_full_frame_drain() {
    let _guard = PROFILER_LOCK
        .lock()
        .expect("profiler lock should not poison");
    let mut grid = DamageGrid::new(17, 78, 100);
    let backend = SocketBackend::new(78, 17);
    let mut terminal = Terminal::new(backend).expect("SocketBackend construction should not fail");
    let area = Rect::new(0, 0, 78, 17);
    let mut output = Vec::with_capacity(8192);

    grid.process(styled_rows("warmup").as_bytes());
    {
        let patch = grid.dump_dirty_patch();
        terminal.backend_mut().draw_grid_patch(area, &patch);
    }
    terminal.backend_mut().drain_output_into(&mut output);
    output.clear();

    grid.process(styled_rows("full frame").as_bytes());
    {
        let snap = grid.dump();
        terminal
            .draw(|frame| frame.render_widget(PaneBodyWidget::new(&snap), area))
            .expect("SocketBackend draw should not fail");
    }
    terminal.backend_mut().drain_output_into(&mut output);
    output.clear();

    grid.process(styled_rows("direct patch").as_bytes());

    let _profiler = dhat::Profiler::builder().testing().build();
    let before = dhat::HeapStats::get();

    {
        let patch = grid.dump_dirty_patch();
        terminal.backend_mut().draw_grid_patch(area, &patch);
    }
    terminal.backend_mut().drain_output_into(&mut output);
    std::hint::black_box(output.len());

    let after = dhat::HeapStats::get();
    dhat::assert_eq!(after.total_blocks - before.total_blocks, 0);
    dhat::assert_eq!(after.total_bytes - before.total_bytes, 0);
}

fn styled_rows(label: &str) -> String {
    let mut input = String::new();
    for row in 1..=17 {
        input.push_str("\x1b[");
        input.push_str(&row.to_string());
        input.push_str(";1H");
        if row % 2 == 0 {
            input.push_str("\x1b[48;2;42;42;42m\x1b[38;2;220;220;220m");
        } else {
            input.push_str("\x1b[0m");
        }
        input.push_str(label);
        input.push(' ');
        input.push_str("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
    }
    input
}
