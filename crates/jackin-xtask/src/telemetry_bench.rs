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
    let mut benchmarks = BTreeMap::new();
    for (name, relative) in SOURCES {
        let path = root.join(relative);
        let estimate: CriterionEstimate = serde_json::from_slice(
            &fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
        )
        .with_context(|| format!("parsing {}", path.display()))?;
        benchmarks.insert((*name).to_owned(), estimate.median.point_estimate);
    }
    let current = Measurements {
        max_regression_percent: baseline.max_regression_percent,
        unit: baseline.unit,
        reference: "captured Criterion medians".to_owned(),
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
    for (name, baseline_value) in &baseline.benchmarks {
        let current_value = current
            .benchmarks
            .get(name)
            .with_context(|| format!("current results omit {name}"))?;
        let limit = baseline_value * (1.0 + baseline.max_regression_percent / 100.0);
        if *current_value > limit {
            bail!(
                "{name} regressed {:.2}% (baseline {baseline_value:.2}, current {current_value:.2}, limit {:.2}%)",
                (current_value / baseline_value - 1.0) * 100.0,
                baseline.max_regression_percent
            );
        }
    }
    Ok(())
}

fn read_measurements(path: &Path) -> Result<Measurements> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("reading {}", path.display()))?)
        .with_context(|| format!("parsing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparator_rejects_doctored_six_percent_regression() {
        let dir = tempfile::tempdir().unwrap();
        let baseline = dir.path().join("baseline.json");
        let current = dir.path().join("current.json");
        fs::write(
            &baseline,
            r#"{"max_regression_percent":5.0,"unit":"ns","benchmarks":{"render":100.0}}"#,
        )
        .unwrap();
        fs::write(
            &current,
            r#"{"max_regression_percent":5.0,"unit":"ns","benchmarks":{"render":106.0}}"#,
        )
        .unwrap();
        assert!(compare(&baseline, &current).is_err());
    }
}
