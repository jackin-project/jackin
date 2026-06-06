#![cfg(feature = "dhat-heap")]

use jackin_capsule::tui::components::pane::PaneBodyWidget;
use jackin_capsule::tui::socket_backend::SocketBackend;
use jackin_term::DamageGrid;
use ratatui::{Terminal, layout::Rect};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[test]
fn focused_dirty_patch_render_core_allocation_stays_bounded_after_warmup() {
    let mut grid = DamageGrid::new(3, 20, 100);
    let backend = SocketBackend::new(20, 3);
    let mut terminal = Terminal::new(backend).expect("SocketBackend construction should not fail");
    let area = Rect::new(0, 0, 20, 3);
    let mut output = Vec::with_capacity(4096);

    grid.process(b"\x1b[1;1Hfirst row\x1b[2;1Hsecond row");
    {
        let patch = grid.dump_dirty_patch();
        terminal
            .draw(|frame| frame.render_widget(PaneBodyWidget::from_patch(&patch), area))
            .expect("SocketBackend draw should not fail");
    }
    terminal.backend_mut().drain_output_into(&mut output);
    output.clear();

    let _profiler = dhat::Profiler::builder().testing().build();
    let before = dhat::HeapStats::get();

    grid.process(b"\x1b[2;1Hchanged");
    {
        let patch = grid.dump_dirty_patch();
        terminal
            .draw(|frame| frame.render_widget(PaneBodyWidget::from_patch(&patch), area))
            .expect("SocketBackend draw should not fail");
    }
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
        bytes <= 512,
        "expected only Ratatui Buffer::diff allocations"
    );
}
