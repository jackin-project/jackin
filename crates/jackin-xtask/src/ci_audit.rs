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
    examples: Vec<String>,
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
            steps.push(StepRow {
                job: job.name.clone(),
                name: step.name,
                result: step.conclusion.unwrap_or(step.status),
                seconds,
            });
        }
        let markers = logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&job.id)
            .map(|bytes| scan_log(&String::from_utf8_lossy(&bytes)))
            .unwrap_or_default();
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
        match cmd::output(Command::new("gh").args(["api", endpoint])) {
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
    let output = cmd::output_string(Command::new("date").args(["-u", "-d", timestamp, "+%s"]))?;
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
    for raw in log.lines() {
        let line = strip_ansi(raw);
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
        let cache_miss = line.contains("Cache not found")
            || line.contains("No cache found")
            || line.contains("Rust cache miss")
            || line.contains("not found for input keys:");
        markers.downloads += usize::from(download);
        markers.builds += usize::from(build);
        markers.source_tools += usize::from(source_tool);
        markers.cache_misses += usize::from(cache_miss);
        if (download || build || source_tool || cache_miss) && markers.examples.len() < 10 {
            markers.examples.push(line);
        }
    }
    markers
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
    }
    total
}

impl Markers {
    fn total(&self) -> usize {
        self.downloads + self.builds + self.source_tools + self.cache_misses
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
        "- Dependency/toolchain download markers: {}\n- Third-party compile/check/build markers: {}\n- Source-tool compile markers: {}\n- Cache-miss markers: {}\n\n",
        totals.downloads, totals.builds, totals.source_tools, totals.cache_misses
    ));
    text.push_str("| Job | Result | Admission | Runtime | Longest step | Downloads | Third-party builds | Tool builds | Cache misses |\n| --- | --- | ---: | ---: | --- | ---: | ---: | ---: | ---: |\n");
    for row in rows {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} ({}) | {} | {} | {} | {} |\n",
            escaped(&row.name),
            row.result,
            duration(row.queue_seconds),
            duration(row.job_seconds),
            escaped(&row.longest_step),
            duration(row.longest_step_seconds),
            row.markers.downloads,
            row.markers.builds,
            row.markers.source_tools,
            row.markers.cache_misses
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

fn duration(seconds: i64) -> String {
    format!("{}m {:02}s", seconds / 60, seconds % 60)
}

fn escaped(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}
