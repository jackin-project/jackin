// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Headless Defect 52 terminal performance experiment runner.
//!
//! This does not replace the Defect 54 live capsule smoke ledger. It captures
//! machine-doable measurements from the owned jackin-term grid, emits governed
//! measurement events, and prints an invocation id for correlating the headless
//! part of Experiments 1-4.

use std::{
    process::Command,
    time::{Duration, Instant},
};

use jackin_core::JackinPaths;
use jackin_diagnostics::RunDiagnostics;
use jackin_term::{DamageGrid, GridPatch};
use serde_json::json;

const ROWS: u16 = 40;
const COLS: u16 = 120;
const SCROLLBACK: usize = 10_000;

#[derive(Debug)]
struct Dataset {
    name: &'static str,
    frames: Vec<Vec<u8>>,
}

#[derive(Debug)]
struct Measurement {
    dataset: &'static str,
    frames: usize,
    jackin_dirty_p99_us: u128,
    jackin_full_dump_p99_us: u128,
    jackin_text_dump_p99_us: u128,
    jackin_changed_cells_total: usize,
    jackin_patch_bytes_estimate: usize,
    jackin_text_bytes_total: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct ProcessSample {
    rss_kb: Option<u64>,
    cpu_percent: Option<f64>,
}

fn seq_dataset() -> Dataset {
    let frames = (1..=400).map(|n| format!("{n}\r\n").into_bytes()).collect();
    Dataset {
        name: "seq_1_100000_window",
        frames,
    }
}

fn agent_dataset() -> Dataset {
    let frames = (0..160)
        .map(|n| {
            format!(
                "\x1b[2J\x1b[HClaude Code\r\nturn={n:03}\r\n\x1b[10;1Hresponse line {n:03}\x1b[0K"
            )
            .into_bytes()
        })
        .collect();
    Dataset {
        name: "agent_response_tall_pane",
        frames,
    }
}

fn redraw_storm_dataset() -> Dataset {
    let frames = (0..120)
        .map(|frame| {
            let mut out = Vec::new();
            out.extend_from_slice(b"\x1b[2J\x1b[H");
            for row in 1..=ROWS {
                out.extend_from_slice(
                    format!("\x1b[{row};1Hframe={frame:03} row={row:02} status=running").as_bytes(),
                );
            }
            out
        })
        .collect();
    Dataset {
        name: "full_screen_redraw_storm",
        frames,
    }
}

fn yes_dataset() -> Dataset {
    let frames = (0..300).map(|_| b"y\r\n".to_vec()).collect();
    Dataset {
        name: "yes_5s_pathological_proxy",
        frames,
    }
}

fn patch_changed_cells(patch: &GridPatch<'_>) -> usize {
    patch
        .changed_spans()
        .map(|(_, _, cells)| cells.iter().filter(|cell| cell.has_contents()).count())
        .sum()
}

fn patch_bytes_estimate(patch: &GridPatch<'_>) -> usize {
    patch
        .changed_spans()
        .map(|(row_idx, start_col, cells)| {
            let cursor_move = format!("\x1b[{};{}H", row_idx + 1, start_col + 1).len();
            let row_bytes = cells
                .iter()
                .map(|cell| cell.contents().len())
                .sum::<usize>();
            cursor_move + row_bytes
        })
        .sum()
}

fn percentile_us(samples: &[Duration], percentile: usize) -> u128 {
    let mut micros = samples.iter().map(Duration::as_micros).collect::<Vec<_>>();
    micros.sort_unstable();
    let Some(last) = micros.len().checked_sub(1) else {
        return 0;
    };
    let idx = (last * percentile).div_ceil(100);
    micros.get(idx).copied().unwrap_or(0)
}

#[expect(
    clippy::disallowed_methods,
    reason = "headless perf example samples its own process metrics outside render/runtime threads"
)]
fn sample_process() -> ProcessSample {
    let pid = std::process::id().to_string();
    let Ok(output) = Command::new("ps")
        .args(["-o", "rss=", "-o", "%cpu=", "-p", &pid])
        .output()
    else {
        return ProcessSample::default();
    };
    if !output.status.success() {
        return ProcessSample::default();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut fields = text.split_whitespace();
    let rss_kb = fields.next().and_then(|v| v.parse().ok());
    let cpu_percent = fields.next().and_then(|v| v.parse().ok());
    ProcessSample {
        rss_kb,
        cpu_percent,
    }
}

fn measure_dataset(dataset: &Dataset) -> Measurement {
    let mut grid = DamageGrid::new(ROWS, COLS, SCROLLBACK);

    let mut jackin_dirty = Vec::with_capacity(dataset.frames.len());
    let mut jackin_full = Vec::with_capacity(dataset.frames.len());
    let mut jackin_text_dump = Vec::with_capacity(dataset.frames.len());
    let mut jackin_changed_cells_total = 0usize;
    let mut jackin_patch_bytes_estimate = 0usize;
    let mut jackin_text_bytes_total = 0usize;

    for frame in &dataset.frames {
        let start = Instant::now();
        grid.process(frame);
        let patch = grid.dump_dirty_patch();
        jackin_dirty.push(start.elapsed());
        jackin_changed_cells_total += patch_changed_cells(&patch);
        jackin_patch_bytes_estimate += patch_bytes_estimate(&patch);

        let start = Instant::now();
        let snapshot = grid.dump();
        std::hint::black_box(snapshot);
        jackin_full.push(start.elapsed());

        let start = Instant::now();
        let contents = grid.dump().to_text();
        jackin_text_bytes_total += contents.len();
        std::hint::black_box(contents);
        jackin_text_dump.push(start.elapsed());
    }

    Measurement {
        dataset: dataset.name,
        frames: dataset.frames.len(),
        jackin_dirty_p99_us: percentile_us(&jackin_dirty, 99),
        jackin_full_dump_p99_us: percentile_us(&jackin_full, 99),
        jackin_text_dump_p99_us: percentile_us(&jackin_text_dump, 99),
        jackin_changed_cells_total,
        jackin_patch_bytes_estimate,
        jackin_text_bytes_total,
    }
}

fn measure_multipane() -> serde_json::Value {
    let pane_counts = [1usize, 4, 8, 16, 32];
    let mut results = Vec::new();
    for panes in pane_counts {
        let before = sample_process();
        let mut grids = (0..panes)
            .map(|_| DamageGrid::new(ROWS, COLS, SCROLLBACK))
            .collect::<Vec<_>>();
        let start = Instant::now();
        for frame in 0..800 {
            for (idx, grid) in grids.iter_mut().enumerate() {
                grid.process(format!("pane={idx:02} frame={frame:03}\r\n").as_bytes());
                std::hint::black_box(grid.dump_dirty_patch());
            }
        }
        let elapsed = start.elapsed();
        let after = sample_process();
        let frames_per_pane = 800usize;
        results.push(json!({
            "panes": panes,
            "frames_per_pane": frames_per_pane,
            "total_us": elapsed.as_micros(),
            "per_frame_us": elapsed.as_micros() / ((panes * frames_per_pane) as u128),
            "model_cells": panes * ROWS as usize * COLS as usize,
            "rss_before_kb": before.rss_kb,
            "rss_after_kb": after.rss_kb,
            "rss_delta_kb": before
                .rss_kb
                .zip(after.rss_kb)
                .map(|(before, after)| after.saturating_sub(before)),
            "cpu_percent_before": before.cpu_percent,
            "cpu_percent_after": after.cpu_percent,
        }));
    }
    json!(results)
}

fn measurement_json(measurement: &Measurement) -> serde_json::Value {
    json!({
        "dataset": measurement.dataset,
        "frames": measurement.frames,
        "jackin_dirty_p99_us": measurement.jackin_dirty_p99_us,
        "jackin_full_dump_p99_us": measurement.jackin_full_dump_p99_us,
        "jackin_text_dump_p99_us": measurement.jackin_text_dump_p99_us,
        "jackin_changed_cells_total": measurement.jackin_changed_cells_total,
        "jackin_patch_bytes_estimate": measurement.jackin_patch_bytes_estimate,
        "jackin_text_bytes_total": measurement.jackin_text_bytes_total,
    })
}

#[expect(
    clippy::print_stdout,
    reason = "example runner must print the invocation id for checklist evidence"
)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::current_dir()?.join("target/jackin-term-perf-runs");
    let paths = JackinPaths::for_tests(&root);
    let run = RunDiagnostics::start(
        &paths,
        false,
        "jackin-term-perf-experiments",
        jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT,
    )?;
    let _guard = run.activate();

    let datasets = [
        seq_dataset(),
        agent_dataset(),
        redraw_storm_dataset(),
        yes_dataset(),
    ];
    let measurements = datasets.iter().map(measure_dataset).collect::<Vec<_>>();
    for measurement in &measurements {
        run.compact(
            "terminal_perf_measurement",
            &format!("{} {}", measurement.dataset, measurement_json(measurement)),
        );
    }
    run.compact(
        "terminal_perf_measurement",
        &format!("multipane_scaling_headless {}", measure_multipane()),
    );
    run.emit_run_summary();

    println!("run_id={}", run.run_id());
    println!("invocation_id={}", run.run_id());
    Ok(())
}
