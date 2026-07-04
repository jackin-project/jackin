// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Frame-time threshold test for the host console render path.
//!
//! Runs one console frame at 220×50 and asserts it completes within the
//! 60 Hz budget (16 ms). This runs in `cargo nextest` / CI so regressions
//! are caught without the variance of a full criterion statistical run.
//!
//! The threshold is intentionally generous (16 ms vs ~0.3 ms actual) so CI
//! runners with variable load don't produce false failures. For exact timing
//! run `cargo bench --bench console_frame -p jackin`.

use std::time::Instant;

use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use jackin::console::tui::{ManagerState, prepare_for_render, render};
use jackin_config::AppConfig;

/// 60 Hz = ~16.7 ms per frame; 16 ms is the hard ceiling.
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

    // Warm-up render — prime allocations so the measured pass is not dominated
    // by first-time malloc cost.
    {
        let mut state = ManagerState::from_config(&config, cwd);
        prepare_for_render(&mut state, &config, cwd, area);
        terminal
            .draw(|frame| render(frame, area, &state, &config, cwd))
            .unwrap();
    }

    // Measured render.
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
