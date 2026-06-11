//! Post-hoc summaries for run diagnostics JSONL artifacts.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::Context;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticsSummary {
    pub run_id: Option<String>,
    pub event_count: u64,
    pub event_counts: BTreeMap<String, u64>,
    pub first_ts_ms: Option<u128>,
    pub last_ts_ms: Option<u128>,
    pub stage_durations_ms: BTreeMap<String, Vec<u64>>,
    pub timing_durations_ms: BTreeMap<String, Vec<u64>>,
    pub docker_build_steps: Vec<DockerBuildStepSummary>,
    pub cache_events: Vec<CacheEventSummary>,
    pub launch_plan_events: Vec<LaunchPlanEventSummary>,
}

impl DiagnosticsSummary {
    #[must_use]
    pub fn wall_duration_ms(&self) -> Option<u128> {
        Some(self.last_ts_ms?.saturating_sub(self.first_ts_ms?))
    }

    #[must_use]
    pub fn cache_hits(&self) -> usize {
        self.cache_events
            .iter()
            .filter(|event| event.kind.contains("cache_hit"))
            .count()
    }

    #[must_use]
    pub fn cache_misses(&self) -> usize {
        self.cache_events
            .iter()
            .filter(|event| event.kind.contains("cache_miss"))
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerBuildStepSummary {
    pub step: String,
    pub label: String,
    pub duration_ms: Option<u64>,
    pub cached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEventSummary {
    pub kind: String,
    pub stage: Option<String>,
    pub message: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlanEventSummary {
    pub kind: String,
    pub plan: Option<String>,
    pub reason: Option<String>,
    pub container: Option<String>,
    pub state: Option<String>,
}

pub fn summarize_run_file(path: &Path) -> anyhow::Result<DiagnosticsSummary> {
    #[expect(
        clippy::disallowed_methods,
        reason = "diagnostics summary is a plain CLI file-inspection path, not a render/runtime thread"
    )]
    let file =
        File::open(path).with_context(|| format!("opening diagnostics run {}", path.display()))?;
    summarize_reader(BufReader::new(file))
}

pub fn summarize_reader(reader: impl BufRead) -> anyhow::Result<DiagnosticsSummary> {
    let mut summary = DiagnosticsSummary {
        run_id: None,
        event_count: 0,
        event_counts: BTreeMap::new(),
        first_ts_ms: None,
        last_ts_ms: None,
        stage_durations_ms: BTreeMap::new(),
        timing_durations_ms: BTreeMap::new(),
        docker_build_steps: Vec::new(),
        cache_events: Vec::new(),
        launch_plan_events: Vec::new(),
    };

    for (line_index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("reading diagnostics line {}", line_index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(&line)
            .with_context(|| format!("parsing diagnostics JSONL line {}", line_index + 1))?;
        summary.event_count += 1;

        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        *summary.event_counts.entry(kind.to_owned()).or_default() += 1;

        if summary.run_id.is_none() {
            summary.run_id = value
                .get("run_id")
                .and_then(Value::as_str)
                .filter(|run_id| !run_id.is_empty())
                .map(ToOwned::to_owned);
        }

        if let Some(ts) = value.get("ts_ms").and_then(Value::as_u64) {
            let ts = u128::from(ts);
            summary.first_ts_ms = Some(summary.first_ts_ms.map_or(ts, |first| first.min(ts)));
            summary.last_ts_ms = Some(summary.last_ts_ms.map_or(ts, |last| last.max(ts)));
        }

        let stage = value
            .get("stage")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let message = value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let detail_raw = value
            .get("detail")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let detail_json = detail_raw
            .as_deref()
            .and_then(|detail| serde_json::from_str::<Value>(detail).ok());

        match kind {
            "stage_done" => {
                if let (Some(stage), Some(duration_ms)) = (
                    stage.as_deref(),
                    detail_json
                        .as_ref()
                        .and_then(|detail| detail.get("duration_ms"))
                        .and_then(Value::as_u64),
                ) {
                    summary
                        .stage_durations_ms
                        .entry(stage.to_owned())
                        .or_default()
                        .push(duration_ms);
                }
            }
            "timing_done" => {
                if let (Some(stage), Some(detail)) = (stage.as_deref(), detail_json.as_ref()) {
                    let name = detail
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    if let Some(duration_ms) = detail.get("duration_ms").and_then(Value::as_u64) {
                        summary
                            .timing_durations_ms
                            .entry(format!("{stage}/{name}"))
                            .or_default()
                            .push(duration_ms);
                    }
                }
            }
            "docker_build_step" => {
                if let Some(detail) = detail_json.as_ref() {
                    summary.docker_build_steps.push(DockerBuildStepSummary {
                        step: detail
                            .get("step")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        label: detail
                            .get("label")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        duration_ms: detail.get("duration_ms").and_then(Value::as_u64),
                        cached: detail
                            .get("cached")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    });
                }
            }
            _ if kind.contains("cache_hit") || kind.contains("cache_miss") => {
                summary.cache_events.push(CacheEventSummary {
                    kind: kind.to_owned(),
                    stage,
                    message,
                    detail: detail_raw,
                });
            }
            "launch_plan" | "launch_plan_rejected" => {
                summary.launch_plan_events.push(LaunchPlanEventSummary {
                    kind: kind.to_owned(),
                    plan: detail_json
                        .as_ref()
                        .and_then(|detail| detail.get("plan"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    reason: detail_json
                        .as_ref()
                        .and_then(|detail| detail.get("reason"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    container: detail_json
                        .as_ref()
                        .and_then(|detail| detail.get("container"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    state: detail_json
                        .as_ref()
                        .and_then(|detail| detail.get("state"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                });
            }
            _ => {}
        }
    }

    Ok(summary)
}
