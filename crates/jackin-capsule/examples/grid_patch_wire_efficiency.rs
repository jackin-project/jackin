//! Production direct-grid-patch wire-efficiency runner.
//!
//! Measures `SocketBackend::draw_grid_patch()` output bytes against the
//! theoretical minimum for the changed cell spans: one cursor move per changed
//! span plus the UTF-8 bytes for the emitted cells. The datasets use default
//! styling so SGR is not part of the minimum.

use jackin_capsule::tui::socket_backend::SocketBackend;
use jackin_core::JackinPaths;
use jackin_diagnostics::RunDiagnostics;
use jackin_term::{Cell, DamageGrid, GridPatch};
use ratatui::layout::Rect;
use serde_json::json;

const ROWS: u16 = 8;
const COLS: u16 = 48;
const SCROLLBACK: usize = 256;

#[derive(Debug)]
struct Dataset {
    name: &'static str,
    warmup: &'static [u8],
    frames: &'static [&'static [u8]],
}

#[derive(Debug)]
struct Measurement {
    dataset: &'static str,
    frames: usize,
    changed_cells: usize,
    actual_bytes: usize,
    theoretical_bytes: usize,
    overhead_percent: f64,
}

fn datasets() -> [Dataset; 4] {
    [
        Dataset {
            name: "single_cell_updates",
            warmup: b"\x1b[1;1Habcdefghijklmnopqrstuvwxyz",
            frames: &[b"\x1b[1;3HZ", b"\x1b[1;7HQ", b"\x1b[1;11HR", b"\x1b[1;15HS"],
        },
        Dataset {
            name: "short_line_rewrites",
            warmup: b"\x1b[2;1Hstatus: idle",
            frames: &[
                b"\x1b[2;9Hbusy",
                b"\x1b[2;9Hdone",
                b"\x1b[2;9Hwait",
                b"\x1b[2;9Hidle",
            ],
        },
        Dataset {
            name: "tail_erases",
            warmup: b"\x1b[3;1Hdownload progress 100 percent",
            frames: &[b"\x1b[3;19H\x1b[8X", b"\x1b[3;19H25", b"\x1b[3;21H\x1b[6X"],
        },
        Dataset {
            name: "multi_row_agent_delta",
            warmup: b"\x1b[4;1Hrole architect\x1b[5;1Hstate running\x1b[6;1Htoken 000",
            frames: &[
                b"\x1b[4;6Hreviewer\x1b[5;7Hqueued\x1b[6;7H001",
                b"\x1b[4;6Hbuilder \x1b[5;7Hactive\x1b[6;7H002",
            ],
        },
    ]
}

fn theoretical_patch_bytes(patch: &GridPatch<'_>) -> (usize, usize) {
    let mut bytes = 0usize;
    let mut cells = 0usize;
    for (row, start_col, span) in patch.changed_spans() {
        bytes += cursor_move_len(row, start_col);
        for cell in span {
            if cell.is_wide_continuation {
                continue;
            }
            bytes += emitted_cell_len(cell);
            cells += 1;
        }
    }
    (bytes, cells)
}

fn cursor_move_len(row: u16, col: u16) -> usize {
    // ESC [ <row+1> ; <col+1> H
    4 + decimal_len(row + 1) + decimal_len(col + 1)
}

fn decimal_len(value: u16) -> usize {
    if value >= 100 {
        3
    } else if value >= 10 {
        2
    } else {
        1
    }
}

fn emitted_cell_len(cell: &Cell) -> usize {
    if cell.contents().is_empty() {
        1
    } else {
        cell.contents().len()
    }
}

fn measure_dataset(dataset: &Dataset) -> Measurement {
    let mut grid = DamageGrid::new(ROWS, COLS, SCROLLBACK);
    let mut backend = SocketBackend::new(COLS, ROWS);
    let area = Rect::new(0, 0, COLS, ROWS);

    grid.process(dataset.warmup);
    {
        let patch = grid.dump_dirty_patch();
        backend.draw_grid_patch(area, &patch);
    }
    drop(backend.take_output());

    let mut actual_bytes = 0usize;
    let mut theoretical_bytes = 0usize;
    let mut changed_cells = 0usize;
    for frame in dataset.frames {
        grid.process(frame);
        let patch = grid.dump_dirty_patch();
        let (frame_theoretical, frame_cells) = theoretical_patch_bytes(&patch);
        theoretical_bytes += frame_theoretical;
        changed_cells += frame_cells;
        backend.draw_grid_patch(area, &patch);
        actual_bytes += backend.take_output().len();
    }
    let overhead_percent = if theoretical_bytes == 0 {
        0.0
    } else {
        (((actual_bytes as f64) / (theoretical_bytes as f64)) - 1.0) * 100.0
    };

    Measurement {
        dataset: dataset.name,
        frames: dataset.frames.len(),
        changed_cells,
        actual_bytes,
        theoretical_bytes,
        overhead_percent,
    }
}

fn measurement_json(measurement: &Measurement) -> serde_json::Value {
    json!({
        "dataset": measurement.dataset,
        "frames": measurement.frames,
        "changed_cells": measurement.changed_cells,
        "actual_bytes": measurement.actual_bytes,
        "theoretical_bytes": measurement.theoretical_bytes,
        "overhead_percent": measurement.overhead_percent,
        "threshold_percent": 15.0,
        "passed": measurement.overhead_percent <= 15.0,
    })
}

#[expect(
    clippy::print_stdout,
    reason = "example runner must print diagnostics run id for checklist evidence"
)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::current_dir()?.join("target/jackin-capsule-wire-runs");
    let paths = JackinPaths::for_tests(&root);
    let run = RunDiagnostics::start(&paths, false, "grid-patch-wire-efficiency")?;
    let _guard = run.activate();

    let mut failed = false;
    for dataset in datasets() {
        let measurement = measure_dataset(&dataset);
        failed |= measurement.overhead_percent > 15.0;
        run.stage(
            "grid_patch_wire_efficiency",
            measurement.dataset,
            "production SocketBackend bytes vs changed-cell theoretical minimum",
            Some(&measurement_json(&measurement).to_string()),
        );
        println!(
            "{}: actual={} theoretical={} overhead={:.2}% changed_cells={}",
            measurement.dataset,
            measurement.actual_bytes,
            measurement.theoretical_bytes,
            measurement.overhead_percent,
            measurement.changed_cells
        );
    }
    run.emit_run_summary();

    println!("run_id={}", run.run_id());
    println!("run_log={}", run.path().display());
    if failed {
        return Err("wire overhead exceeded 15% threshold".into());
    }
    Ok(())
}
