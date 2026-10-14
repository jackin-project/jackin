use std::time::Instant;

use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use jackin::console::adapter::{ManagerState, prepare_for_render, render};
use jackin_config::AppConfig;

const FRAME_BUDGET_MS: u128 = 16;

#[test]
fn console_list_frame_220x50_completes_within_60hz_budget() {
    let config = AppConfig::default();
    let cwd = std::path::Path::new("/workspace/jackin-project/jackin");
    let area = Rect {
        x: 0,
        y: 0,
        width: 220,
        height: 50,
    };
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();

    {
        let mut state = ManagerState::from_config(&config, cwd);
        prepare_for_render(&mut state, &config, cwd, area);
        terminal
            .draw(|frame| render(frame, area, &state, &config, cwd))
            .unwrap();
    }

    let mut state = ManagerState::from_config(&config, cwd);
    prepare_for_render(&mut state, &config, cwd, area);
    let start = Instant::now();
    terminal
        .draw(|frame| render(frame, area, &state, &config, cwd))
        .unwrap();
    let elapsed = start.elapsed().as_millis();

    assert!(
        elapsed < FRAME_BUDGET_MS,
        "console frame took {elapsed}ms — exceeds {FRAME_BUDGET_MS}ms 60Hz budget. \
         Run `cargo bench --bench console_frame -p jackin` for detailed timing."
    );
}
