// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Deserialize;

use crate::cmd;

#[cfg(test)]
mod tests;

type JobLogs = Arc<Mutex<BTreeMap<u64, Vec<u8>>>>;

#[derive(Args, Debug)]
pub(crate) struct CiAuditArgs {
    /// Fail when a warm run emits dependency, build, tool, or cache-miss markers.
    #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
    expect_clean: bool,
    /// Human-readable workflow label used in the step summary.
    #[arg(long, default_value = "CI")]
    workflow_label: String,
    #[arg(long)]
    repository: Option<String>,
    #[arg(long)]
    run_id: Option<u64>,
    #[arg(long)]
    run_attempt: Option<u64>,
    #[arg(long)]
    summary: Option<String>,
}

#[derive(Deserialize)]
struct Run {
    created_at: String,
}

#[derive(Deserialize)]
struct JobsResponse {
    jobs: Vec<Job>,
}

#[derive(Deserialize)]
struct Job {
    id: u64,
    name: String,
    status: String,
    conclusion: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
struct Step {
    name: String,
    status: String,
    conclusion: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
}

#[derive(Deserialize)]
struct VelnorReport {
    cache_outcomes: Option<CacheOutcomes>,
    compiler: Option<CompilerReport>,
}

#[derive(Deserialize)]
struct CacheOutcomes {
    #[serde(flatten)]
    layers: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct CompilerReport {
    #[serde(default)]
    compiling_lines: usize,
    #[serde(default)]
    mbx_outcomes: Vec<String>,
}

struct Row {
    name: String,
    result: String,
    queue_seconds: i64,
    job_seconds: i64,
    longest_step: String,
    longest_step_seconds: i64,
    markers: Markers,
}

struct StepRow {
    job: String,
    name: String,
    result: String,
    seconds: i64,
}

#[derive(Default)]
struct Markers {
    downloads: usize,
    builds: usize,
    source_tools: usize,
    cache_misses: usize,
    cache_exact_hits: usize,
    cache_partial_restores: usize,
    report_count: usize,
    report_parse_errors: usize,
    reported_cache_exact: usize,
    reported_cache_non_exact: usize,
    reported_cache_inactive: usize,
    reported_cache_unknown: usize,
    reported_compiler_lines: usize,
    mbx_object_hits: usize,
    mbx_object_misses: usize,
    reported_cache_layers: BTreeMap<String, String>,
    product: ProductMarkers,
    examples: Vec<String>,
}

#[derive(Default)]
struct ProductMarkers {
    staged: usize,
    uploaded: usize,
    downloaded: usize,
    verified: usize,
    non_success: usize,
}

pub(crate) fn run(args: CiAuditArgs) -> Result<()> {
    let repository = required(args.repository, "GITHUB_REPOSITORY")?;
    let run_id = args.run_id.map_or_else(
        || {
            required(None, "GITHUB_RUN_ID")?
                .parse()
                .context("parsing GITHUB_RUN_ID")
        },
        Ok,
    )?;
    let summary = required(args.summary, "GITHUB_STEP_SUMMARY")?;
    let run_attempt = args.run_attempt.map_or_else(
        || {
            required(None, "GITHUB_RUN_ATTEMPT")?
                .parse()
                .context("parsing GITHUB_RUN_ATTEMPT")
        },
        Ok,
    )?;
    let run: Run = api_json(&format!("repos/{repository}/actions/runs/{run_id}"))?;
    let jobs: JobsResponse = api_json(&format!(
        "repos/{repository}/actions/runs/{run_id}/attempts/{run_attempt}/jobs?per_page=100"
    ))?;
    let run_created = epoch(&run.created_at)?;
    let logs = download_logs(&repository, &jobs.jobs)?;
    let mut rows = Vec::new();
    let mut steps = Vec::new();

    for job in jobs.jobs {
        let started = epoch_optional(job.started_at.as_deref())?;
        let completed = epoch_optional(job.completed_at.as_deref())?;
        let job_seconds = elapsed(started, completed);
        let queue_seconds = if started > run_created {
            started - run_created
        } else {
            0
        };
        let mut markers = logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&job.id)
            .map(|bytes| scan_log(&String::from_utf8_lossy(&bytes)))
            .unwrap_or_default();
        let mut longest_step = String::from("-");
        let mut longest_step_seconds = 0;
        for step in job.steps {
            let seconds = elapsed(
                epoch_optional(step.started_at.as_deref())?,
                epoch_optional(step.completed_at.as_deref())?,
            );
            if seconds > longest_step_seconds {
                longest_step_seconds = seconds;
                longest_step.clone_from(&step.name);
            }
            let result = step.conclusion.unwrap_or(step.status);
            markers.product.observe(&step.name, &result);
            steps.push(StepRow {
                job: job.name.clone(),
                name: step.name,
                result,
                seconds,
            });
        }
        rows.push(Row {
            name: job.name,
            result: job.conclusion.unwrap_or(job.status),
            queue_seconds,
            job_seconds,
            longest_step,
            longest_step_seconds,
            markers,
        });
    }

    let totals = totals(&rows);
    append_summary(
        Path::new(&summary),
        &args.workflow_label,
        &rows,
        &steps,
        &totals,
    )?;
    if args.expect_clean && totals.total() != 0 {
        bail!("warm run emitted forbidden cache/dependency/tool markers");
    }
    Ok(())
}

fn download_logs(repository: &str, jobs: &[Job]) -> Result<JobLogs> {
    let logs = Arc::new(Mutex::new(BTreeMap::new()));
    let errors = Arc::new(Mutex::new(Vec::new()));
    std::thread::scope(|scope| {
        for job in jobs
            .iter()
            .filter(|job| job.status == "completed" && job.conclusion.as_deref() != Some("skipped"))
        {
            let logs = Arc::clone(&logs);
            let errors = Arc::clone(&errors);
            let endpoint = format!("repos/{repository}/actions/jobs/{}/logs", job.id);
            scope.spawn(move || match api_bytes(&endpoint) {
                Ok(bytes) => {
                    logs.lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .insert(job.id, bytes);
                }
                Err(error) => errors
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(format!("{}: {error:#}", job.name)),
            });
        }
    });
    let errors = errors
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if errors.is_empty() {
        Ok(logs)
    } else {
        bail!("failed to download job logs:\n  {}", errors.join("\n  "))
    }
}

fn required(value: Option<String>, environment: &str) -> Result<String> {
    value
        .or_else(|| env::var(environment).ok())
        .with_context(|| format!("{environment} must be set"))
}

fn api_json<T: for<'de> Deserialize<'de>>(endpoint: &str) -> Result<T> {
    let mut last = None;
    for attempt in 0..4 {
        match api_bytes(endpoint).and_then(|bytes| {
            serde_json::from_slice(&bytes).context("GitHub API returned a non-JSON response")
        }) {
            Ok(value) => return Ok(value),
            Err(error) => last = Some(error),
        }
        backoff(attempt);
    }
    Err(last.context("GitHub API request did not run")?)
}

fn api_bytes(endpoint: &str) -> Result<Vec<u8>> {
    let mut last = None;
    for attempt in 0..4 {
        match cmd::output(Command::new("gh").args(["api", "--allow-escape-sequences", endpoint])) {
            Ok(bytes) if !bytes.starts_with(b"<") => return Ok(bytes),
            Ok(_) => last = Some(anyhow::anyhow!("GitHub API returned HTML")),
            Err(error) => last = Some(error),
        }
        backoff(attempt);
    }
    Err(last.context("GitHub API request did not run")?)
}

fn backoff(attempt: usize) {
    let pair = (Mutex::new(()), Condvar::new());
    let guard = pair
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    drop(
        pair.1
            .wait_timeout(guard, Duration::from_millis(250 * (attempt as u64 + 1)))
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
}

fn epoch(timestamp: &str) -> Result<i64> {
    if timestamp.is_empty() || timestamp == "null" || timestamp.starts_with("0001-") {
        return Ok(0);
    }
    let without_zone = timestamp.strip_suffix('Z').unwrap_or(timestamp);
    let normalized = without_zone
        .split_once('.')
        .map_or(without_zone, |(whole_seconds, _)| whole_seconds);
    let output = if cfg!(target_os = "macos") {
        cmd::output_string(Command::new("date").args([
            "-u",
            "-j",
            "-f",
            "%Y-%m-%dT%H:%M:%S",
            normalized,
            "+%s",
        ]))?
    } else {
        cmd::output_string(Command::new("date").args([
            "-u",
            "-d",
            &format!("{normalized}Z"),
            "+%s",
        ]))?
    };
    output
        .trim()
        .parse()
        .with_context(|| format!("parsing timestamp `{timestamp}`"))
}

fn epoch_optional(timestamp: Option<&str>) -> Result<i64> {
    timestamp.map_or(Ok(0), epoch)
}

fn elapsed(started: i64, completed: i64) -> i64 {
    if started > 0 && completed > started {
        completed - started
    } else {
        0
    }
}

fn scan_log(log: &str) -> Markers {
    let mut markers = Markers::default();
    let mut pending_exact_cache_key = None;
    for raw in log.lines() {
        let line = strip_ansi(raw);
        scan_velnor_report(&line, &mut markers);
        let cache_hit = marker_value(&line, "Cache hit for:");
        let cache_restore = marker_value(&line, "Cache restored from key:");
        let download = line.contains("crates.io index")
            || line.contains("Downloading crates")
            || (line.contains("Downloaded ") && line.contains(" v"))
            || (line.contains("info: downloading ") && line.contains(" components"));
        let trimmed = line.trim();
        let build = ["Compiling ", "Checking ", "Building "]
            .iter()
            .any(|prefix| trimmed.starts_with(prefix))
            && trimmed.contains(" v")
            && !trimmed.contains(" (/");
        let source_tool =
            line.contains("Installing ") && line.contains(" v") && line.contains("from source");
        let lower = line.to_ascii_lowercase();
        let cache_miss = lower.contains("cache not found")
            || lower.contains("no cache found")
            || lower.contains("rust cache miss")
            || lower.contains("not found for input keys:");
        if let Some(key) = cache_hit.as_deref() {
            markers.cache_exact_hits += 1;
            pending_exact_cache_key = Some(key.to_owned());
        } else if let Some(key) = cache_restore.as_deref() {
            if pending_exact_cache_key.as_deref() != Some(key) {
                markers.cache_partial_restores += 1;
            }
            pending_exact_cache_key = None;
        } else if cache_miss {
            pending_exact_cache_key = None;
        }
        markers.downloads += usize::from(download);
        markers.builds += usize::from(build);
        markers.source_tools += usize::from(source_tool);
        markers.cache_misses += usize::from(cache_miss);
        if (download
            || build
            || source_tool
            || cache_miss
            || cache_hit.is_some()
            || cache_restore.is_some())
            && markers.examples.len() < 10
        {
            markers.examples.push(line);
        }
    }
    markers
}

fn scan_velnor_report(line: &str, markers: &mut Markers) {
    const PREFIX: &str = "VELNOR_CI_REPORT ";
    let Some(start) = line.find(PREFIX) else {
        return;
    };
    let payload = line[start + PREFIX.len()..].trim();
    if !payload.starts_with('{') {
        return;
    }
    match serde_json::from_str::<VelnorReport>(payload) {
        Ok(report) => record_velnor_report(report, markers),
        Err(_) => markers.report_parse_errors += 1,
    }
}

fn record_velnor_report(report: VelnorReport, markers: &mut Markers) {
    markers.report_count += 1;
    if let Some(cache_outcomes) = report.cache_outcomes {
        for (layer, outcome) in cache_outcomes.layers {
            if layer == "lane" {
                continue;
            }
            match outcome.to_ascii_lowercase().as_str() {
                "exact" => markers.reported_cache_exact += 1,
                "cold" | "miss" | "partial" | "compatible_seed" | "warm" => {
                    markers.reported_cache_non_exact += 1;
                }
                "disabled" | "not_run" => markers.reported_cache_inactive += 1,
                _ => markers.reported_cache_unknown += 1,
            }
            markers.reported_cache_layers.insert(layer, outcome);
        }
    }
    if let Some(compiler) = report.compiler {
        markers.reported_compiler_lines += compiler.compiling_lines;
        for outcome in compiler.mbx_outcomes {
            markers.mbx_object_hits += metric_count(&outcome, "object cache:");
            markers.mbx_object_misses += metric_count(&outcome, "hits,");
        }
    }
}

fn metric_count(value: &str, marker: &str) -> usize {
    let lower = value.to_ascii_lowercase();
    let Some(start) = lower.find(&marker.to_ascii_lowercase()) else {
        return 0;
    };
    value[start + marker.len()..]
        .split_whitespace()
        .next()
        .and_then(|number| number.parse().ok())
        .unwrap_or(0)
}

fn marker_value(line: &str, marker: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let start = lower.find(&marker.to_ascii_lowercase())?;
    Some(line[start + marker.len()..].trim().to_owned())
}

fn strip_ansi(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' && chars.next_if_eq(&'[').is_some() {
            for code in chars.by_ref() {
                if code.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            output.push(character);
        }
    }
    output
}

fn totals(rows: &[Row]) -> Markers {
    let mut total = Markers::default();
    for row in rows {
        total.downloads += row.markers.downloads;
        total.builds += row.markers.builds;
        total.source_tools += row.markers.source_tools;
        total.cache_misses += row.markers.cache_misses;
        total.cache_exact_hits += row.markers.cache_exact_hits;
        total.cache_partial_restores += row.markers.cache_partial_restores;
        total.report_count += row.markers.report_count;
        total.report_parse_errors += row.markers.report_parse_errors;
        total.reported_cache_exact += row.markers.reported_cache_exact;
        total.reported_cache_non_exact += row.markers.reported_cache_non_exact;
        total.reported_cache_inactive += row.markers.reported_cache_inactive;
        total.reported_cache_unknown += row.markers.reported_cache_unknown;
        total.reported_compiler_lines += row.markers.reported_compiler_lines;
        total.mbx_object_hits += row.markers.mbx_object_hits;
        total.mbx_object_misses += row.markers.mbx_object_misses;
        total.product.staged += row.markers.product.staged;
        total.product.uploaded += row.markers.product.uploaded;
        total.product.downloaded += row.markers.product.downloaded;
        total.product.verified += row.markers.product.verified;
        total.product.non_success += row.markers.product.non_success;
    }
    total
}

impl Markers {
    fn total(&self) -> usize {
        self.downloads
            + self.builds
            + self.source_tools
            + self.cache_misses
            + self.report_parse_errors
            + self.reported_cache_non_exact
            + self.reported_cache_unknown
            + self.mbx_object_misses
    }
}

impl ProductMarkers {
    fn observe(&mut self, name: &str, result: &str) {
        let name = name.to_ascii_lowercase();
        let kind = if name.contains("stage product") {
            Some(&mut self.staged)
        } else if name.contains("upload product") {
            Some(&mut self.uploaded)
        } else if name.contains("download product") {
            Some(&mut self.downloaded)
        } else if name.contains("verify product") {
            Some(&mut self.verified)
        } else {
            None
        };
        if let Some(count) = kind {
            *count += 1;
            if result != "success" {
                self.non_success += 1;
            }
        }
    }
}

fn append_summary(
    path: &Path,
    label: &str,
    rows: &[Row],
    steps: &[StepRow],
    totals: &Markers,
) -> Result<()> {
    let mut text = String::new();
    text.push_str(&format!("### {label} performance audit\n\n"));
    text.push_str(&format!(
        "- Dependency/toolchain download markers: {}\n- Third-party compile/check/build markers: {}\n- Source-tool compile markers: {}\n- Cache log outcomes: {} exact hits, {} partial restores, {} misses\n- Structured Velnor reports: {} ({} parse errors)\n- Reported cache layers: {} exact, {} non-exact, {} inactive, {} unknown\n- Reported compiler lines: {}\n- Mr. Boxington object cache: {} hits, {} misses\n- Product transport steps: {} staged, {} uploaded, {} downloaded, {} verified, {} non-success\n\n",
        totals.downloads,
        totals.builds,
        totals.source_tools,
        totals.cache_exact_hits,
        totals.cache_partial_restores,
        totals.cache_misses,
        totals.report_count,
        totals.report_parse_errors,
        totals.reported_cache_exact,
        totals.reported_cache_non_exact,
        totals.reported_cache_inactive,
        totals.reported_cache_unknown,
        totals.reported_compiler_lines,
        totals.mbx_object_hits,
        totals.mbx_object_misses,
        totals.product.staged,
        totals.product.uploaded,
        totals.product.downloaded,
        totals.product.verified,
        totals.product.non_success
    ));
    text.push_str("| Job | Result | Admission | Runtime | Longest step | Downloads | Third-party builds | Tool builds | Cache log H/P/M | Reported cache | Compiler lines | MBX H/M | Product S/U/D/V/! |\n| --- | --- | ---: | ---: | --- | ---: | ---: | ---: | --- | --- | ---: | --- | --- |\n");
    for row in rows {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} ({}) | {} | {} | {} | {}/{}/{} | {} | {} | {}/{} | {}/{}/{}/{}/{} |\n",
            escaped(&row.name),
            row.result,
            duration(row.queue_seconds),
            duration(row.job_seconds),
            escaped(&row.longest_step),
            duration(row.longest_step_seconds),
            row.markers.downloads,
            row.markers.builds,
            row.markers.source_tools,
            row.markers.cache_exact_hits,
            row.markers.cache_partial_restores,
            row.markers.cache_misses,
            escaped(&reported_cache(&row.markers)),
            row.markers.reported_compiler_lines,
            row.markers.mbx_object_hits,
            row.markers.mbx_object_misses,
            row.markers.product.staged,
            row.markers.product.uploaded,
            row.markers.product.downloaded,
            row.markers.product.verified,
            row.markers.product.non_success
        ));
    }
    text.push_str("\n<details><summary>Every step duration</summary>\n\n| Job | Step | Result | Duration |\n| --- | --- | --- | ---: |\n");
    for step in steps {
        text.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            escaped(&step.job),
            escaped(&step.name),
            step.result,
            duration(step.seconds)
        ));
    }
    text.push_str("\n</details>\n");
    let examples = rows
        .iter()
        .flat_map(|row| {
            row.markers
                .examples
                .iter()
                .map(move |line| (&row.name, line))
        })
        .take(30)
        .collect::<Vec<_>>();
    if !examples.is_empty() {
        text.push_str(
            "\n<details><summary>First cache/download/build markers</summary>\n\n```text\n",
        );
        for (job, line) in examples {
            text.push_str(&format!("{job}\t{line}\n"));
        }
        text.push_str("```\n</details>\n");
    }
    let mut existing = fs::read_to_string(path).unwrap_or_default();
    existing.push_str(&text);
    fs::write(path, existing).with_context(|| format!("writing {}", path.display()))
}

fn reported_cache(markers: &Markers) -> String {
    if markers.reported_cache_layers.is_empty() {
        return String::from("-");
    }
    markers
        .reported_cache_layers
        .iter()
        .map(|(layer, outcome)| format!("{layer}={outcome}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn duration(seconds: i64) -> String {
    format!("{}m {:02}s", seconds / 60, seconds % 60)
}

fn escaped(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}
