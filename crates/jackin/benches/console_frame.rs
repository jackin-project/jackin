//! Frame-time benchmark for the host console TUI compose path.
//!
//! Budget: the host console should compose a representative workspace
//! frame well inside one 60 Hz frame (~16ms). Run with:
//!
//! ```sh
//! cargo bench --bench console_frame
//! ```

use criterion::{Criterion, criterion_group, criterion_main};
use ratatui::{Terminal, backend::TestBackend};

use jackin::console::tui::{ManagerState, prepare_for_render, render};
use jackin_config::AppConfig;

fn build_list_state(config: &AppConfig) -> ManagerState<'_> {
    let cwd = std::path::Path::new("/workspace/jackin-project/jackin");
    ManagerState::from_config(config, cwd)
}

fn bench_list_render(c: &mut Criterion) {
    let config = AppConfig::default();
    let cwd = std::path::Path::new("/workspace/jackin-project/jackin");
    let area = ratatui::layout::Rect {
        x: 0,
        y: 0,
        width: 220,
        height: 50,
    };
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();

    c.bench_function("console_list_frame_220x50", |b| {
        b.iter(|| {
            let mut state = build_list_state(&config);
            prepare_for_render(&mut state, &config, cwd, area);
            terminal
                .draw(|frame| render(frame, area, &state, &config, cwd))
                .unwrap();
        });
    });
}

criterion_group!(benches, bench_list_render);
criterion_main!(benches);
