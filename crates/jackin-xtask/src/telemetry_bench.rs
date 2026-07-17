// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Reviewed Criterion baseline capture and comparison for governed telemetry.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::{Deserialize, Serialize};

use crate::docs::repo_root;

const BASELINE: &str = "crates/jackin-telemetry/benches/baseline.json";
const CURRENT: &str = "target/telemetry-bench-current.json";
const CALIBRATION: &str = "target/criterion/telemetry_calibration/new/estimates.json";
const SOURCES: &[(&str, &str)] = &[
    (
        "launch_pipeline",
        "target/criterion/launch_pipeline_run_launch_core_e2e_fakedocker/new/estimates.json",
    ),
    (
        "console_frame",
        "target/criterion/console_list_frame_220x50/new/estimates.json",
    ),
    (
        "pane_body",
        "target/criterion/pane_body/custom_widget_ratatui/200x50/new/estimates.json",
    ),
    (
        "pty_byte_pump",
        "target/criterion/pty_byte_pump_4k_with_telemetry/new/estimates.json",
    ),
];

#[derive(Args, Debug)]
pub(crate) struct TelemetryBenchArgs {
    /// Capture Criterion medians into the current-results file before comparing.
    #[arg(long)]
    capture: bool,
    /// Override the reviewed baseline JSON.
    #[arg(long, default_value = BASELINE)]
    baseline: PathBuf,
    /// Override the current-results JSON.
    #[arg(long, default_value = CURRENT)]
    current: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
struct Measurements {
    max_regression_percent: f64,
    unit: String,
    #[serde(default)]
    reference: String,
    calibration: f64,
    benchmarks: BTreeMap<String, f64>,
}

#[derive(Deserialize)]
struct CriterionEstimate {
    median: PointEstimate,
}

#[derive(Deserialize)]
struct PointEstimate {
    point_estimate: f64,
}

pub(crate) fn run(args: TelemetryBenchArgs) -> Result<()> {
    let root = repo_root()?;
    let baseline_path = root.join(args.baseline);
    let current_path = root.join(args.current);
    if args.capture {
        capture(&root, &baseline_path, &current_path)?;
    }
    compare(&baseline_path, &current_path)
}

fn capture(root: &Path, baseline_path: &Path, current_path: &Path) -> Result<()> {
    let baseline = read_measurements(baseline_path)?;
    let calibration = read_median(&root.join(CALIBRATION))?;
    let mut benchmarks = BTreeMap::new();
    for (name, relative) in SOURCES {
        benchmarks.insert((*name).to_owned(), read_median(&root.join(relative))?);
    }
    let current = Measurements {
        max_regression_percent: baseline.max_regression_percent,
        unit: baseline.unit,
        reference: "same-run Criterion medians normalized by telemetry_calibration".to_owned(),
        calibration,
        benchmarks,
    };
    if let Some(parent) = current_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(current_path, serde_json::to_vec_pretty(&current)?)?;
    Ok(())
}

fn compare(baseline_path: &Path, current_path: &Path) -> Result<()> {
    let baseline = read_measurements(baseline_path)?;
    let current = read_measurements(current_path)?;
    if baseline.unit != current.unit {
        bail!("telemetry benchmark units differ");
    }
    validate_measurements("baseline", &baseline)?;
    validate_measurements("current", &current)?;
    for (name, baseline_value) in &baseline.benchmarks {
        let current_value = current
            .benchmarks
            .get(name)
            .with_context(|| format!("current results omit {name}"))?;
        let baseline_ratio = baseline_value / baseline.calibration;
        let current_ratio = current_value / current.calibration;
        let limit = baseline_ratio * (1.0 + baseline.max_regression_percent / 100.0);
        if current_ratio > limit {
            bail!(
                "{name} regressed {:.2}% after calibration (baseline ratio {baseline_ratio:.6}, current ratio {current_ratio:.6}, limit {:.2}%)",
                (current_ratio / baseline_ratio - 1.0) * 100.0,
                baseline.max_regression_percent
            );
        }
    }
    Ok(())
}

fn read_median(path: &Path) -> Result<f64> {
    let estimate: CriterionEstimate = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("reading {}", path.display()))?,
    )
    .with_context(|| format!("parsing {}", path.display()))?;
    Ok(estimate.median.point_estimate)
}

fn validate_measurements(label: &str, measurements: &Measurements) -> Result<()> {
    if !measurements.calibration.is_finite() || measurements.calibration <= 0.0 {
        bail!("{label} calibration must be finite and positive");
    }
    for (name, value) in &measurements.benchmarks {
        if !value.is_finite() || *value <= 0.0 {
            bail!("{label} benchmark {name} must be finite and positive");
        }
    }
    Ok(())
}

fn read_measurements(path: &Path) -> Result<Measurements> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("reading {}", path.display()))?)
        .with_context(|| format!("parsing {}", path.display()))
}

#[cfg(test)]
mod tests;
