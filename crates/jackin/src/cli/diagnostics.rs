//! CLI support for inspecting run diagnostics artifacts.

use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::{Args, Subcommand, ValueEnum};

use jackin_core::JackinPaths;

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum DiagnosticsCommand {
    /// Summarize a diagnostics run JSONL artifact
    Summary(DiagnosticsSummaryArgs),
    /// Compare multiple diagnostics run JSONL artifacts
    Compare(DiagnosticsCompareArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct DiagnosticsSummaryArgs {
    /// Run ID (`jk-run-...`) or path to a diagnostics JSONL file.
    pub run: String,
    /// Number of slow stage/timing/build rows to print per section.
    #[arg(long, default_value_t = 10)]
    pub top: usize,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct DiagnosticsCompareArgs {
    /// Run IDs (`jk-run-...`) or paths to diagnostics JSONL files.
    #[arg(required = true, num_args = 2..)]
    pub runs: Vec<String>,
    /// Number of slow stage/timing rows to print per section.
    #[arg(long, default_value_t = 10)]
    pub top: usize,
    /// Startup baseline for per-run deltas.
    #[arg(long, value_enum, default_value = "fastest")]
    pub baseline: DiagnosticsCompareBaseline,
    /// Output format.
    #[arg(long, value_enum, default_value = "text")]
    pub format: DiagnosticsCompareFormat,
    /// Write JSON output to this explicit path instead of stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Display labels for the supplied runs. Repeat once per run.
    #[arg(long = "label")]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum DiagnosticsCompareBaseline {
    /// Compare each run to the fastest startup span.
    Fastest,
    /// Compare each run to the first supplied run.
    First,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum DiagnosticsCompareFormat {
    /// Human-readable comparison tables.
    Text,
    /// Machine-readable rows for cold/warm/restart timing archives.
    Json,
}

pub fn run(command: &DiagnosticsCommand, paths: &JackinPaths) -> anyhow::Result<()> {
    match command {
        DiagnosticsCommand::Summary(args) => summary(args, paths),
        DiagnosticsCommand::Compare(args) => compare(args, paths),
    }
}

fn summary(args: &DiagnosticsSummaryArgs, paths: &JackinPaths) -> anyhow::Result<()> {
    let path = resolve_run_path(paths, &args.run);
    let summary = jackin_diagnostics::summarize_run_file(&path)?;
    print_summary(&summary, &path, args.top);
    Ok(())
}

fn compare(args: &DiagnosticsCompareArgs, paths: &JackinPaths) -> anyhow::Result<()> {
    validate_compare_args(args)?;
    let runs = args
        .runs
        .iter()
        .map(|run| {
            let path = resolve_run_path(paths, run);
            let summary = jackin_diagnostics::summarize_run_file(&path)?;
            Ok((path, summary))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    match args.format {
        DiagnosticsCompareFormat::Text => {
            print_comparison(&runs, args.top, args.baseline, &args.labels);
        }
        DiagnosticsCompareFormat::Json => {
            let output = render_comparison_json(&runs, args.baseline, &args.labels)?;
            if let Some(path) = args.output.as_deref() {
                write_compare_output(path, &output)?;
            } else {
                println!("{output}");
            }
        }
    }
    Ok(())
}

fn validate_compare_args(args: &DiagnosticsCompareArgs) -> anyhow::Result<()> {
    if args.output.is_some() && args.format != DiagnosticsCompareFormat::Json {
        anyhow::bail!("--output requires --format json");
    }
    if !args.labels.is_empty() && args.labels.len() != args.runs.len() {
        anyhow::bail!("--label must be supplied once per run when used");
    }
    Ok(())
}

fn render_comparison_json(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    baseline: DiagnosticsCompareBaseline,
    labels: &[String],
) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(&comparison_json(
        runs, baseline, labels,
    ))?)
}

fn write_compare_output(path: &Path, output: &str) -> anyhow::Result<()> {
    fs::write(path, format!("{output}\n")).map_err(|error| {
        anyhow::anyhow!(
            "failed to write diagnostics comparison artifact {}: {error}",
            path.display()
        )
    })
}

fn resolve_run_path(paths: &JackinPaths, run: &str) -> PathBuf {
    let candidate = Path::new(run);
    if candidate.exists()
        || run.contains(std::path::MAIN_SEPARATOR)
        || candidate
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
    {
        return candidate.to_path_buf();
    }
    paths
        .data_dir
        .join("diagnostics")
        .join("runs")
        .join(format!("{run}.jsonl"))
}

fn print_summary(summary: &jackin_diagnostics::DiagnosticsSummary, path: &Path, top: usize) {
    println!("Run: {}", summary.run_id.as_deref().unwrap_or("(unknown)"));
    println!("File: {}", path.display());
    println!("Events: {}", summary.event_count);
    if let Some(duration_ms) = summary.wall_duration_ms() {
        println!("Timeline: {}", format_duration(duration_ms));
    }
    if let Some(duration_ms) = summary.startup_duration_ms() {
        println!("Startup: {}", format_duration(duration_ms));
    }
    println!(
        "Cache: {} hit(s), {} miss(es)",
        summary.cache_hits(),
        summary.cache_misses()
    );

    print_duration_section("Stages", stage_rows(&summary.stage_durations_ms), top);
    print_duration_section("Timings", stage_rows(&summary.timing_durations_ms), top);
    print_skipped_timing_section(summary, top);
    print_launch_plan_section(summary, top);
    print_prewarmed_dind_section(summary, top);
    print_build_context_section(summary, top);
    print_build_section(summary, top);
    print_cache_section(summary, top);
}

fn print_skipped_timing_section(summary: &jackin_diagnostics::DiagnosticsSummary, top: usize) {
    println!();
    println!("Skipped Timings");
    if summary.skipped_timings.is_empty() {
        println!("  (none)");
        return;
    }
    for timing in summary.skipped_timings.iter().take(top) {
        println!(
            "  {:<42} {}",
            truncate_name(&format!("{}/{}", timing.stage, timing.name), 42),
            timing.detail
        );
    }
}

fn print_launch_plan_section(summary: &jackin_diagnostics::DiagnosticsSummary, top: usize) {
    println!();
    println!("Launch Plans");
    if summary.launch_plan_events.is_empty() {
        println!("  (none)");
        return;
    }
    for event in summary.launch_plan_events.iter().take(top) {
        let status = match event.kind.as_str() {
            "launch_plan" => "selected",
            "launch_plan_rejected" => "rejected",
            other => other,
        };
        let plan = event.plan.as_deref().unwrap_or("(unknown)");
        let reason = event.reason.as_deref().unwrap_or("(no reason)");
        let container = event.container.as_deref().unwrap_or("-");
        let state = event.state.as_deref().unwrap_or("-");
        println!("  {status:<8} {plan:<22} {reason:<36} {container:<28} {state}");
    }
}

fn print_prewarmed_dind_section(summary: &jackin_diagnostics::DiagnosticsSummary, top: usize) {
    println!();
    println!("Prewarmed DinD Adoption");
    if summary.prewarmed_dind_adoptions.is_empty() {
        println!("  (none)");
        return;
    }
    for event in summary.prewarmed_dind_adoptions.iter().take(top) {
        println!(
            "  {:<8} {}",
            event.outcome,
            format_prewarmed_dind_adoption_detail(event)
        );
    }
}

fn format_prewarmed_dind_adoption_detail(
    event: &jackin_diagnostics::PrewarmedDindAdoptionSummary,
) -> String {
    let mut parts = Vec::new();
    if let Some(reason) = event.reason.as_deref() {
        parts.push(format!("reason={reason}"));
    }
    if let Some(source) = event.source.as_deref() {
        parts.push(format!("source={source}"));
    }
    if let Some(ready_ms) = event.ready_ms {
        parts.push(format!("ready_ms={ready_ms}"));
    }
    if let Some(prewarm_ready_ms) = event.prewarm_ready_ms {
        parts.push(format!("prewarm_ready_ms={prewarm_ready_ms}"));
    }
    if let Some(state_age_ms) = event.state_age_ms {
        parts.push(format!("state_age_ms={state_age_ms}"));
    }
    if parts.is_empty() {
        event.detail.clone().unwrap_or_else(|| "-".to_owned())
    } else {
        parts.join(" ")
    }
}

fn print_comparison(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    top: usize,
    baseline: DiagnosticsCompareBaseline,
    labels: &[String],
) {
    println!("Runs");
    let startup_baseline = startup_baseline_duration(runs, baseline);
    for (index, (path, summary)) in runs.iter().enumerate() {
        let label = comparison_label_with_override(index, path, summary, labels);
        let timeline = summary
            .wall_duration_ms()
            .map_or_else(|| "(unknown)".to_owned(), format_duration);
        let startup = summary
            .startup_duration_ms()
            .map_or_else(|| "(unknown)".to_owned(), format_duration);
        let startup_delta = format_startup_delta(summary.startup_duration_ms(), startup_baseline);
        println!(
            "  {label}: startup {startup} ({startup_delta}), timeline {timeline}, {} event(s), {} cache hit(s), {} cache miss(es)",
            summary.event_count,
            summary.cache_hits(),
            summary.cache_misses(),
        );
    }

    print_startup_spread(runs, labels);
    print_comparison_section("Stage Comparison", runs, top, labels, |summary| {
        &summary.stage_durations_ms
    });
    print_comparison_section("Timing Comparison", runs, top, labels, |summary| {
        &summary.timing_durations_ms
    });
    print_skipped_timing_comparison(runs, top, labels);
    print_launch_plan_comparison(runs, labels);
    print_prewarmed_dind_comparison(runs, labels);
    print_build_context_comparison(runs, labels);
    print_docker_build_step_comparison(runs, top, labels);
    print_cache_comparison(runs, labels);
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StartupSpreadSummary {
    fastest_label: String,
    fastest_ms: u128,
    slowest_label: String,
    slowest_ms: u128,
    spread_ms: u128,
}

fn print_startup_spread(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    labels: &[String],
) {
    println!();
    println!("Startup Spread");
    let Some(spread) = startup_spread_summary(runs, labels) else {
        println!("  (no startup spans)");
        return;
    };
    println!(
        "  fastest {:<28} {}",
        truncate_name(&spread.fastest_label, 28),
        format_duration(spread.fastest_ms)
    );
    println!(
        "  slowest {:<28} {}",
        truncate_name(&spread.slowest_label, 28),
        format_duration(spread.slowest_ms)
    );
    println!("  spread  {}", format_duration(spread.spread_ms));
}

fn startup_spread_summary(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    labels: &[String],
) -> Option<StartupSpreadSummary> {
    let mut fastest: Option<(String, u128)> = None;
    let mut slowest: Option<(String, u128)> = None;
    for (index, (path, summary)) in runs.iter().enumerate() {
        let Some(startup_ms) = summary.startup_duration_ms() else {
            continue;
        };
        let label = comparison_label_with_override(index, path, summary, labels);
        if fastest
            .as_ref()
            .is_none_or(|(_, fastest_ms)| startup_ms < *fastest_ms)
        {
            fastest = Some((label.clone(), startup_ms));
        }
        if slowest
            .as_ref()
            .is_none_or(|(_, slowest_ms)| startup_ms > *slowest_ms)
        {
            slowest = Some((label, startup_ms));
        }
    }
    let (fastest_label, fastest_ms) = fastest?;
    let (slowest_label, slowest_ms) = slowest?;
    Some(StartupSpreadSummary {
        fastest_label,
        fastest_ms,
        slowest_label,
        slowest_ms,
        spread_ms: slowest_ms.saturating_sub(fastest_ms),
    })
}

fn comparison_json(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    baseline: DiagnosticsCompareBaseline,
    labels: &[String],
) -> serde_json::Value {
    let startup_baseline = startup_baseline_duration(runs, baseline);
    let baseline_name = match baseline {
        DiagnosticsCompareBaseline::Fastest => "fastest",
        DiagnosticsCompareBaseline::First => "first",
    };
    let rows = runs
        .iter()
        .enumerate()
        .map(|(index, (path, summary))| {
            let selected_plan = selected_launch_plan(summary);
            serde_json::json!({
                "label": comparison_label_with_override(index, path, summary, labels),
                "run_id": summary.run_id.as_deref(),
                "path": path.display().to_string(),
                "startup_ms": summary.startup_duration_ms(),
                "timeline_ms": summary.wall_duration_ms(),
                "startup_delta": format_startup_delta(summary.startup_duration_ms(), startup_baseline),
                "startup_delta_ms": startup_delta_ms(summary.startup_duration_ms(), startup_baseline),
                "startup_saved_ms": startup_saved_ms(summary.startup_duration_ms(), startup_baseline),
                "startup_ratio": startup_ratio(summary.startup_duration_ms(), startup_baseline),
                "event_count": summary.event_count,
                "cache_hits": summary.cache_hits(),
                "cache_misses": summary.cache_misses(),
                "selected_plan": selected_plan.and_then(|event| event.plan.as_deref()),
                "selected_reason": selected_plan.and_then(|event| event.reason.as_deref()),
                "selected_container": selected_plan.and_then(|event| event.container.as_deref()),
                "launch_plan_events": launch_plan_events_json(summary),
                "prewarmed_dind_adoptions": prewarmed_dind_adoptions_json(summary),
                "build_context_snapshots": build_context_snapshots_json(summary),
                "image_build_sources": image_build_sources_json(summary),
                "max_build_context_bytes": max_build_context_bytes(summary),
                "max_build_context_files": max_build_context_files(summary),
                "stage_durations_ms": &summary.stage_durations_ms,
                "timing_durations_ms": &summary.timing_durations_ms,
                "slowest_stage_ms": slowest_named_duration(&summary.stage_durations_ms),
                "slowest_timing_ms": slowest_named_duration(&summary.timing_durations_ms),
                "slowest_docker_build_step_ms": slowest_docker_build_step(summary),
                "docker_build_steps": docker_build_steps_json(summary),
                "cache_decision": cache_decision_json(summary),
                "cache_decisions": cache_decisions_json(summary),
                "skipped_timings": skipped_timing_json(summary),
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "baseline": baseline_name,
        "startup_baseline_ms": startup_baseline,
        "fastest_startup_run": startup_extreme_run(runs, StartupExtreme::Fastest, labels),
        "slowest_startup_run": startup_extreme_run(runs, StartupExtreme::Slowest, labels),
        "startup_spread_ms": startup_spread_ms(runs),
        "selected_plan_counts": selected_plan_counts(runs),
        "cache_decision_counts": cache_decision_counts(runs),
        "prewarmed_dind_adoption_counts": prewarmed_dind_adoption_counts(runs),
        "slowest_stage_ms": slowest_named_duration_across_runs(runs, labels, |summary| &summary.stage_durations_ms),
        "slowest_timing_ms": slowest_named_duration_across_runs(runs, labels, |summary| &summary.timing_durations_ms),
        "slowest_docker_build_step_ms": slowest_docker_build_step_across_runs(runs, labels),
        "runs": rows,
    })
}

enum StartupExtreme {
    Fastest,
    Slowest,
}

fn startup_extreme_run(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    extreme: StartupExtreme,
    labels: &[String],
) -> Option<serde_json::Value> {
    let row = runs
        .iter()
        .enumerate()
        .filter_map(|(index, (path, summary))| {
            summary
                .startup_duration_ms()
                .map(|startup_ms| (index, path, summary, startup_ms))
        });
    let (index, path, summary, startup_ms) = match extreme {
        StartupExtreme::Fastest => row.min_by_key(|(index, path, summary, startup_ms)| {
            (
                *startup_ms,
                comparison_label_with_override(*index, path, summary, labels),
            )
        })?,
        StartupExtreme::Slowest => row.max_by_key(|(index, path, summary, startup_ms)| {
            (
                *startup_ms,
                std::cmp::Reverse(comparison_label_with_override(
                    *index, path, summary, labels,
                )),
            )
        })?,
    };
    Some(serde_json::json!({
        "label": comparison_label_with_override(index, path, summary, labels),
        "run_id": summary.run_id.as_deref(),
        "path": path.display().to_string(),
        "startup_ms": startup_ms,
    }))
}

fn startup_spread_ms(runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)]) -> Option<u128> {
    let mut startup_ms = runs
        .iter()
        .filter_map(|(_, summary)| summary.startup_duration_ms());
    let first = startup_ms.next()?;
    let (min, max) = startup_ms.fold((first, first), |(min, max), value| {
        (min.min(value), max.max(value))
    });
    Some(max - min)
}

fn selected_plan_counts(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for (_, summary) in runs {
        let key = selected_launch_plan(summary)
            .and_then(|event| event.plan.as_deref())
            .unwrap_or("none");
        *counts.entry(key.to_owned()).or_insert(0) += 1;
    }
    counts
}

fn cache_decision_counts(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for (_, summary) in runs {
        let key = summary
            .cache_events
            .first()
            .map_or("none", |event| event.kind.as_str());
        *counts.entry(key.to_owned()).or_insert(0) += 1;
    }
    counts
}

fn prewarmed_dind_adoption_counts(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for (_, summary) in runs {
        let key =
            last_prewarmed_dind_adoption(summary).map_or("none", |event| event.outcome.as_str());
        *counts.entry(key.to_owned()).or_insert(0) += 1;
    }
    counts
}

fn slowest_named_duration_across_runs(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    labels: &[String],
    durations_for: fn(
        &jackin_diagnostics::DiagnosticsSummary,
    ) -> &std::collections::BTreeMap<String, Vec<u64>>,
) -> Option<serde_json::Value> {
    runs.iter()
        .enumerate()
        .flat_map(|(index, (path, summary))| {
            durations_for(summary)
                .iter()
                .filter_map(move |(name, values)| {
                    max_duration(Some(values))
                        .map(|duration_ms| (index, path, summary, name.as_str(), duration_ms))
                })
        })
        .max_by(|left, right| left.4.cmp(&right.4).then_with(|| right.3.cmp(left.3)))
        .map(|(index, path, summary, name, duration_ms)| {
            serde_json::json!({
                "name": name,
                "duration_ms": duration_ms,
                "label": comparison_label_with_override(index, path, summary, labels),
                "run_id": summary.run_id.as_deref(),
                "path": path.display().to_string(),
            })
        })
}

fn slowest_docker_build_step_across_runs(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    labels: &[String],
) -> Option<serde_json::Value> {
    runs.iter()
        .enumerate()
        .flat_map(|(index, (path, summary))| {
            summary.docker_build_steps.iter().filter_map(move |step| {
                step.duration_ms
                    .map(|duration_ms| (index, path, summary, step, duration_ms))
            })
        })
        .max_by(|left, right| {
            left.4
                .cmp(&right.4)
                .then_with(|| right.3.step.cmp(&left.3.step))
        })
        .map(|(index, path, summary, step, duration_ms)| {
            serde_json::json!({
                "name": docker_build_step_name(step),
                "duration_ms": duration_ms,
                "cached": step.cached,
                "label": comparison_label_with_override(index, path, summary, labels),
                "run_id": summary.run_id.as_deref(),
                "path": path.display().to_string(),
            })
        })
}

fn startup_delta_ms(current: Option<u128>, baseline: Option<u128>) -> Option<i64> {
    let current = i128::try_from(current?).ok()?;
    let baseline = i128::try_from(baseline?).ok()?;
    i64::try_from(current - baseline).ok()
}

fn startup_saved_ms(current: Option<u128>, baseline: Option<u128>) -> Option<i64> {
    startup_delta_ms(current, baseline).and_then(i64::checked_neg)
}

fn startup_ratio(current: Option<u128>, baseline: Option<u128>) -> Option<f64> {
    let current = current?;
    let baseline = baseline?;
    if baseline == 0 {
        return None;
    }
    Some((current as f64) / (baseline as f64))
}

fn slowest_docker_build_step(
    summary: &jackin_diagnostics::DiagnosticsSummary,
) -> Option<serde_json::Value> {
    summary
        .docker_build_steps
        .iter()
        .filter_map(|step| step.duration_ms.map(|duration_ms| (step, duration_ms)))
        .max_by(|left, right| {
            left.1
                .cmp(&right.1)
                .then_with(|| right.0.step.cmp(&left.0.step))
        })
        .map(|(step, duration_ms)| {
            serde_json::json!({
                "name": docker_build_step_name(step),
                "duration_ms": duration_ms,
                "cached": step.cached,
            })
        })
}

fn cache_decision_json(
    summary: &jackin_diagnostics::DiagnosticsSummary,
) -> Option<serde_json::Value> {
    summary.cache_events.first().map(cache_event_json)
}

fn cache_decisions_json(
    summary: &jackin_diagnostics::DiagnosticsSummary,
) -> Vec<serde_json::Value> {
    summary.cache_events.iter().map(cache_event_json).collect()
}

fn cache_event_json(event: &jackin_diagnostics::CacheEventSummary) -> serde_json::Value {
    serde_json::json!({
        "decision": event.kind,
        "stage": event.stage,
        "message": event.message,
        "detail": event.detail,
    })
}

fn launch_plan_events_json(
    summary: &jackin_diagnostics::DiagnosticsSummary,
) -> Vec<serde_json::Value> {
    summary
        .launch_plan_events
        .iter()
        .map(|event| {
            serde_json::json!({
                "kind": event.kind,
                "plan": event.plan,
                "reason": event.reason,
                "container": event.container,
                "state": event.state,
            })
        })
        .collect()
}

fn prewarmed_dind_adoptions_json(
    summary: &jackin_diagnostics::DiagnosticsSummary,
) -> Vec<serde_json::Value> {
    summary
        .prewarmed_dind_adoptions
        .iter()
        .map(|event| {
            serde_json::json!({
                "outcome": event.outcome,
                "detail": event.detail,
                "reason": event.reason,
                "source": event.source,
                "ready_ms": event.ready_ms,
                "prewarm_ready_ms": event.prewarm_ready_ms,
                "state_age_ms": event.state_age_ms,
            })
        })
        .collect()
}

fn build_context_snapshots_json(
    summary: &jackin_diagnostics::DiagnosticsSummary,
) -> Vec<serde_json::Value> {
    summary
        .build_context_snapshots
        .iter()
        .map(|snapshot| {
            serde_json::json!({
                "source": snapshot.source,
                "files": snapshot.files,
                "bytes": snapshot.bytes,
                "context_dir": snapshot.context_dir,
            })
        })
        .collect()
}

fn image_build_sources_json(
    summary: &jackin_diagnostics::DiagnosticsSummary,
) -> Vec<serde_json::Value> {
    summary
        .image_build_sources
        .iter()
        .map(|source| {
            serde_json::json!({
                "source": source.source,
                "reason": source.reason,
                "base_image": source.base_image,
                "pull_base_image": source.pull_base_image,
            })
        })
        .collect()
}

fn docker_build_steps_json(
    summary: &jackin_diagnostics::DiagnosticsSummary,
) -> Vec<serde_json::Value> {
    summary
        .docker_build_steps
        .iter()
        .map(|step| {
            serde_json::json!({
                "step": step.step,
                "label": step.label,
                "name": docker_build_step_name(step),
                "duration_ms": step.duration_ms,
                "cached": step.cached,
            })
        })
        .collect()
}

fn skipped_timing_json(summary: &jackin_diagnostics::DiagnosticsSummary) -> Vec<serde_json::Value> {
    summary
        .skipped_timings
        .iter()
        .map(|timing| {
            serde_json::json!({
                "stage": timing.stage,
                "name": timing.name,
                "detail": timing.detail,
            })
        })
        .collect()
}

fn slowest_named_duration(
    durations: &std::collections::BTreeMap<String, Vec<u64>>,
) -> Option<serde_json::Value> {
    durations
        .iter()
        .filter_map(|(name, values)| max_duration(Some(values)).map(|duration| (name, duration)))
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(left.0)))
        .map(|(name, duration_ms)| {
            serde_json::json!({
                "name": name,
                "duration_ms": duration_ms,
            })
        })
}

fn startup_baseline_duration(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    baseline: DiagnosticsCompareBaseline,
) -> Option<u128> {
    match baseline {
        DiagnosticsCompareBaseline::Fastest => runs
            .iter()
            .filter_map(|(_, summary)| summary.startup_duration_ms())
            .min(),
        DiagnosticsCompareBaseline::First => runs
            .first()
            .and_then(|(_, summary)| summary.startup_duration_ms()),
    }
}

fn format_startup_delta(startup_ms: Option<u128>, fastest_ms: Option<u128>) -> String {
    let Some(startup_ms) = startup_ms else {
        return "no startup span".to_owned();
    };
    let Some(fastest_ms) = fastest_ms else {
        return "no baseline".to_owned();
    };
    if startup_ms == fastest_ms {
        return "baseline".to_owned();
    }
    if fastest_ms == 0 {
        let delta = startup_ms.saturating_sub(fastest_ms);
        return format!("+{}", format_duration(delta));
    }
    if startup_ms > fastest_ms {
        let delta = startup_ms.saturating_sub(fastest_ms);
        format!(
            "+{}, {:.1}x slower",
            format_duration(delta),
            (startup_ms as f64) / (fastest_ms as f64)
        )
    } else {
        let delta = fastest_ms.saturating_sub(startup_ms);
        if startup_ms == 0 {
            return format!("-{}", format_duration(delta));
        }
        format!(
            "-{}, {:.1}x faster",
            format_duration(delta),
            (fastest_ms as f64) / (startup_ms as f64)
        )
    }
}

fn print_skipped_timing_comparison(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    top: usize,
    labels: &[String],
) {
    println!();
    println!("Skipped Timing Comparison");
    let names = skipped_timing_names(runs, top);
    if names.is_empty() {
        println!("  (none)");
        return;
    }

    print!("  {:<42}", "name");
    for (index, (path, summary)) in runs.iter().enumerate() {
        print!(
            " {:>10}",
            comparison_label_with_override(index, path, summary, labels)
        );
    }
    println!();

    for name in names {
        print!("  {:<42}", truncate_name(&name, 42));
        for (_, summary) in runs {
            let detail = skipped_timing_detail(summary, &name).unwrap_or("-");
            print!(" {detail:>10}");
        }
        println!();
    }
}

fn skipped_timing_names(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    top: usize,
) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    for (_, summary) in runs {
        for timing in &summary.skipped_timings {
            names.insert(format!("{}/{}", timing.stage, timing.name));
        }
    }
    names.into_iter().take(top).collect()
}

fn skipped_timing_detail<'a>(
    summary: &'a jackin_diagnostics::DiagnosticsSummary,
    name: &str,
) -> Option<&'a str> {
    summary
        .skipped_timings
        .iter()
        .find(|timing| format!("{}/{}", timing.stage, timing.name) == name)
        .map(|timing| timing.detail.as_str())
}

fn print_comparison_section(
    title: &str,
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    top: usize,
    labels: &[String],
    durations: fn(
        &jackin_diagnostics::DiagnosticsSummary,
    ) -> &std::collections::BTreeMap<String, Vec<u64>>,
) {
    println!();
    println!("{title}");
    let names = comparison_names(runs, top, durations);
    if names.is_empty() {
        println!("  (none)");
        return;
    }

    print!("  {:<42}", "name");
    for (index, (path, summary)) in runs.iter().enumerate() {
        print!(
            " {:>10}",
            comparison_label_with_override(index, path, summary, labels)
        );
    }
    println!();

    for name in names {
        print!("  {:<42}", truncate_name(&name, 42));
        for (_, summary) in runs {
            let duration = max_duration(durations(summary).get(&name));
            let formatted =
                duration.map_or_else(|| "-".to_owned(), |ms| format_duration(u128::from(ms)));
            print!(" {formatted:>10}");
        }
        println!();
    }
}

fn comparison_names(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    top: usize,
    durations: fn(
        &jackin_diagnostics::DiagnosticsSummary,
    ) -> &std::collections::BTreeMap<String, Vec<u64>>,
) -> Vec<String> {
    let mut maxima = std::collections::BTreeMap::<String, u64>::new();
    for (_, summary) in runs {
        for (name, values) in durations(summary) {
            let Some(max) = max_duration(Some(values)) else {
                continue;
            };
            maxima
                .entry(name.clone())
                .and_modify(|current| *current = (*current).max(max))
                .or_insert(max);
        }
    }
    let mut rows: Vec<_> = maxima.into_iter().collect();
    rows.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    rows.into_iter().take(top).map(|(name, _)| name).collect()
}

fn print_build_context_comparison(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    labels: &[String],
) {
    println!();
    println!("Build Context Comparison");
    if runs
        .iter()
        .all(|(_, summary)| summary.build_context_snapshots.is_empty())
    {
        println!("  (none)");
        return;
    }

    print!("  {:<42}", "metric");
    for (index, (path, summary)) in runs.iter().enumerate() {
        print!(
            " {:>10}",
            comparison_label_with_override(index, path, summary, labels)
        );
    }
    println!();

    print!("  {:<42}", "max bytes");
    for (_, summary) in runs {
        let formatted =
            max_build_context_bytes(summary).map_or_else(|| "-".to_owned(), format_bytes);
        print!(" {formatted:>10}");
    }
    println!();

    print!("  {:<42}", "max files");
    for (_, summary) in runs {
        let formatted = max_build_context_files(summary)
            .map_or_else(|| "-".to_owned(), |files| files.to_string());
        print!(" {formatted:>10}");
    }
    println!();
}

fn max_build_context_bytes(summary: &jackin_diagnostics::DiagnosticsSummary) -> Option<u64> {
    summary
        .build_context_snapshots
        .iter()
        .map(|snapshot| snapshot.bytes)
        .max()
}

fn max_build_context_files(summary: &jackin_diagnostics::DiagnosticsSummary) -> Option<u64> {
    summary
        .build_context_snapshots
        .iter()
        .map(|snapshot| snapshot.files)
        .max()
}

fn print_launch_plan_comparison(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    labels: &[String],
) {
    println!();
    println!("Launch Plan Comparison");
    if runs
        .iter()
        .all(|(_, summary)| selected_launch_plan(summary).is_none())
    {
        println!("  (none)");
        return;
    }

    println!(
        "  {:<42} {:<22} {:<36} container",
        "run", "selected plan", "reason"
    );
    for (index, (path, summary)) in runs.iter().enumerate() {
        let label = comparison_label_with_override(index, path, summary, labels);
        let Some(event) = selected_launch_plan(summary) else {
            println!(
                "  {:<42} {:<22} {:<36} -",
                truncate_name(&label, 42),
                "-",
                "-"
            );
            continue;
        };
        println!(
            "  {:<42} {:<22} {:<36} {}",
            truncate_name(&label, 42),
            event.plan.as_deref().unwrap_or("-"),
            truncate_name(event.reason.as_deref().unwrap_or("-"), 36),
            event.container.as_deref().unwrap_or("-")
        );
    }
}

fn selected_launch_plan(
    summary: &jackin_diagnostics::DiagnosticsSummary,
) -> Option<&jackin_diagnostics::LaunchPlanEventSummary> {
    summary
        .launch_plan_events
        .iter()
        .find(|event| event.kind == "launch_plan")
}

fn print_prewarmed_dind_comparison(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    labels: &[String],
) {
    println!();
    println!("Prewarmed DinD Adoption Comparison");
    if runs
        .iter()
        .all(|(_, summary)| last_prewarmed_dind_adoption(summary).is_none())
    {
        println!("  (none)");
        return;
    }

    println!("  {:<42} {:<10} summary", "run", "outcome");
    for (index, (path, summary)) in runs.iter().enumerate() {
        let label = comparison_label_with_override(index, path, summary, labels);
        let Some(event) = last_prewarmed_dind_adoption(summary) else {
            println!("  {:<42} {:<10} -", truncate_name(&label, 42), "-");
            continue;
        };
        println!(
            "  {:<42} {:<10} {}",
            truncate_name(&label, 42),
            event.outcome,
            format_prewarmed_dind_adoption_detail(event)
        );
    }
}

fn last_prewarmed_dind_adoption(
    summary: &jackin_diagnostics::DiagnosticsSummary,
) -> Option<&jackin_diagnostics::PrewarmedDindAdoptionSummary> {
    summary.prewarmed_dind_adoptions.last()
}

fn print_docker_build_step_comparison(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    top: usize,
    labels: &[String],
) {
    println!();
    println!("Docker Build Step Comparison");
    let names = docker_build_step_names(runs, top);
    if names.is_empty() {
        println!("  (none)");
        return;
    }

    print!("  {:<42}", "step");
    for (index, (path, summary)) in runs.iter().enumerate() {
        print!(
            " {:>10}",
            comparison_label_with_override(index, path, summary, labels)
        );
    }
    println!();

    for name in names {
        print!("  {:<42}", truncate_name(&name, 42));
        for (_, summary) in runs {
            let formatted = max_docker_build_step_duration(summary, &name)
                .map_or_else(|| "-".to_owned(), |ms| format_duration(u128::from(ms)));
            print!(" {formatted:>10}");
        }
        println!();
    }
}

fn docker_build_step_names(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    top: usize,
) -> Vec<String> {
    let mut maxima = std::collections::BTreeMap::<String, u64>::new();
    for (_, summary) in runs {
        for step in &summary.docker_build_steps {
            let Some(duration_ms) = step.duration_ms else {
                continue;
            };
            let name = docker_build_step_name(step);
            maxima
                .entry(name)
                .and_modify(|current| *current = (*current).max(duration_ms))
                .or_insert(duration_ms);
        }
    }
    let mut rows: Vec<_> = maxima.into_iter().collect();
    rows.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    rows.into_iter().take(top).map(|(name, _)| name).collect()
}

fn max_docker_build_step_duration(
    summary: &jackin_diagnostics::DiagnosticsSummary,
    name: &str,
) -> Option<u64> {
    summary
        .docker_build_steps
        .iter()
        .filter(|step| docker_build_step_name(step) == name)
        .filter_map(|step| step.duration_ms)
        .max()
}

fn docker_build_step_name(step: &jackin_diagnostics::DockerBuildStepSummary) -> String {
    if step.label.is_empty() {
        step.step.clone()
    } else {
        format!("{} {}", step.step, step.label)
    }
}

fn print_cache_comparison(
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    labels: &[String],
) {
    println!();
    println!("Cache Decision Comparison");
    if runs
        .iter()
        .all(|(_, summary)| summary.cache_events.is_empty())
    {
        println!("  (none)");
        return;
    }

    println!("  {:<42} {:<16} {:<18} detail", "run", "decision", "stage");
    for (index, (path, summary)) in runs.iter().enumerate() {
        let label = comparison_label_with_override(index, path, summary, labels);
        let Some(event) = summary.cache_events.first() else {
            println!(
                "  {:<42} {:<16} {:<18} -",
                truncate_name(&label, 42),
                "-",
                "-"
            );
            continue;
        };
        println!(
            "  {:<42} {:<16} {:<18} {}",
            truncate_name(&label, 42),
            event.kind,
            event.stage.as_deref().unwrap_or("-"),
            event.detail.as_deref().unwrap_or(&event.message)
        );
    }
}

fn comparison_label(
    index: usize,
    path: &Path,
    summary: &jackin_diagnostics::DiagnosticsSummary,
) -> String {
    summary.run_id.as_ref().map_or_else(
        || {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| !stem.is_empty())
                .map_or_else(|| format!("run{}", index + 1), ToOwned::to_owned)
        },
        ToOwned::to_owned,
    )
}

fn comparison_label_with_override(
    index: usize,
    path: &Path,
    summary: &jackin_diagnostics::DiagnosticsSummary,
    labels: &[String],
) -> String {
    labels
        .get(index)
        .filter(|label| !label.is_empty())
        .cloned()
        .unwrap_or_else(|| comparison_label(index, path, summary))
}

fn max_duration(values: Option<&Vec<u64>>) -> Option<u64> {
    values.and_then(|values| values.iter().copied().max())
}

fn truncate_name(name: &str, width: usize) -> String {
    let mut chars = name.chars();
    let truncated: String = chars.by_ref().take(width).collect();
    if chars.next().is_none() {
        return truncated;
    }
    let mut shortened: String = name.chars().take(width.saturating_sub(1)).collect();
    shortened.push('…');
    shortened
}

fn print_duration_section(title: &str, mut rows: Vec<(String, u64)>, top: usize) {
    println!();
    println!("{title}");
    if rows.is_empty() {
        println!("  (none)");
        return;
    }
    rows.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    for (name, duration_ms) in rows.into_iter().take(top) {
        println!("  {:>10}  {name}", format_duration(u128::from(duration_ms)));
    }
}

fn print_build_context_section(summary: &jackin_diagnostics::DiagnosticsSummary, top: usize) {
    println!();
    println!("Build Contexts");
    if summary.build_context_snapshots.is_empty() {
        println!("  (none)");
        return;
    }
    let mut rows = summary.build_context_snapshots.clone();
    rows.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| right.files.cmp(&left.files))
    });
    for snapshot in rows.into_iter().take(top) {
        let context_dir = snapshot.context_dir.as_deref().unwrap_or("-");
        let source = snapshot.source.as_deref().unwrap_or("-");
        println!(
            "  {:>10}  {:>8} file(s)  {:<9}  {}",
            format_bytes(snapshot.bytes),
            snapshot.files,
            source,
            context_dir
        );
    }
}

fn print_build_section(summary: &jackin_diagnostics::DiagnosticsSummary, top: usize) {
    println!();
    println!("Docker Build Steps");
    if summary.docker_build_steps.is_empty() {
        println!("  (none)");
        return;
    }
    let mut rows = summary.docker_build_steps.clone();
    rows.sort_by(|left, right| {
        right
            .duration_ms
            .unwrap_or_default()
            .cmp(&left.duration_ms.unwrap_or_default())
            .then_with(|| left.step.cmp(&right.step))
    });
    for step in rows.into_iter().take(top) {
        let duration = step.duration_ms.map_or_else(
            || "(unknown)".to_owned(),
            |ms| format_duration(u128::from(ms)),
        );
        let cache = if step.cached { "cached" } else { "ran" };
        println!(
            "  {:>10}  {:<6} {} {}",
            duration, cache, step.step, step.label
        );
    }
}

fn print_cache_section(summary: &jackin_diagnostics::DiagnosticsSummary, top: usize) {
    println!();
    println!("Cache Decisions");
    if summary.cache_events.is_empty() {
        println!("  (none)");
        return;
    }
    for event in summary.cache_events.iter().take(top) {
        let stage = event.stage.as_deref().unwrap_or("(no stage)");
        let detail = event.detail.as_deref().unwrap_or("");
        if detail.is_empty() {
            println!("  {}  {}  {}", event.kind, stage, event.message);
        } else {
            println!("  {}  {}  {} ({detail})", event.kind, stage, event.message);
        }
    }
}

fn stage_rows(durations: &std::collections::BTreeMap<String, Vec<u64>>) -> Vec<(String, u64)> {
    durations
        .iter()
        .flat_map(|(name, values)| {
            values
                .iter()
                .copied()
                .map(|duration| (name.clone(), duration))
        })
        .collect()
}

fn format_duration(ms: u128) -> String {
    if ms >= 1_000 {
        format!("{:.1}s", (ms as f64) / 1_000.0)
    } else {
        format!("{ms}ms")
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", (bytes as f64) / MIB)
    } else if bytes >= 1024 {
        format!("{:.1} KiB", (bytes as f64) / KIB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DiagnosticsCompareArgs, DiagnosticsCompareBaseline, DiagnosticsCompareFormat,
        comparison_json, comparison_names, docker_build_step_names, format_bytes, format_duration,
        format_startup_delta, last_prewarmed_dind_adoption, max_build_context_bytes,
        max_build_context_files, max_docker_build_step_duration, render_comparison_json,
        resolve_run_path, selected_launch_plan, skipped_timing_detail, skipped_timing_names,
        startup_baseline_duration, startup_spread_summary, truncate_name, validate_compare_args,
        write_compare_output,
    };
    use jackin_core::JackinPaths;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn run_id_resolves_to_diagnostics_jsonl() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());

        let path = resolve_run_path(&paths, "jk-run-abc123");

        assert_eq!(
            path,
            paths
                .data_dir
                .join("diagnostics")
                .join("runs")
                .join("jk-run-abc123.jsonl")
        );
    }

    #[test]
    fn duration_formatter_uses_seconds_after_one_second() {
        assert_eq!(format_duration(999), "999ms");
        assert_eq!(format_duration(1_250), "1.2s");
    }

    #[test]
    fn startup_delta_formatter_compares_to_fastest_run() {
        assert_eq!(format_startup_delta(Some(1_000), Some(1_000)), "baseline");
        assert_eq!(
            format_startup_delta(Some(3_000), Some(1_000)),
            "+2.0s, 3.0x slower"
        );
        assert_eq!(
            format_startup_delta(Some(1_000), Some(3_000)),
            "-2.0s, 3.0x faster"
        );
        assert_eq!(format_startup_delta(None, Some(1_000)), "no startup span");
    }

    #[test]
    fn startup_spread_summary_names_fastest_slowest_and_spread() {
        let runs = vec![
            (PathBuf::from("cold.jsonl"), summary_with_startup(6_000)),
            (PathBuf::from("warm.jsonl"), summary_with_startup(1_250)),
        ];
        let spread = startup_spread_summary(&runs, &[]).unwrap();

        assert_eq!(spread.fastest_label, "warm");
        assert_eq!(spread.fastest_ms, 1_250);
        assert_eq!(spread.slowest_label, "cold");
        assert_eq!(spread.slowest_ms, 6_000);
        assert_eq!(spread.spread_ms, 4_750);
    }

    #[test]
    fn startup_baseline_supports_fastest_and_first_run() {
        let runs = vec![
            (PathBuf::from("cold.jsonl"), summary_with_startup(5_000)),
            (PathBuf::from("warm.jsonl"), summary_with_startup(900)),
            (PathBuf::from("no-hardline.jsonl"), summary_with_stages([])),
        ];

        assert_eq!(
            startup_baseline_duration(&runs, DiagnosticsCompareBaseline::Fastest),
            Some(900)
        );
        assert_eq!(
            startup_baseline_duration(&runs, DiagnosticsCompareBaseline::First),
            Some(5_000)
        );
    }

    #[test]
    fn comparison_json_exports_startup_and_plan_rows() {
        let mut cold = summary_with_startup(5_000);
        cold.run_id = Some("jk-run-cold".to_owned());
        cold.event_count = 42;
        cold.stage_durations_ms
            .insert("derived image".to_owned(), vec![2_000]);
        cold.timing_durations_ms
            .insert("image/docker_build".to_owned(), vec![1_500]);
        cold.build_context_snapshots
            .push(jackin_diagnostics::BuildContextSnapshotSummary {
                source: Some("workspace".to_owned()),
                files: 7,
                bytes: 2048,
                context_dir: Some("/tmp/context".to_owned()),
            });
        cold.image_build_sources
            .push(jackin_diagnostics::ImageBuildSourceSummary {
                source: Some("workspace_dockerfile".to_owned()),
                reason: Some("missing_local_image".to_owned()),
                base_image: None,
                pull_base_image: false,
            });
        cold.launch_plan_events
            .push(jackin_diagnostics::LaunchPlanEventSummary {
                kind: "launch_plan".to_owned(),
                plan: Some("BuildAndCreate".to_owned()),
                reason: Some("missing_local_image".to_owned()),
                container: Some("jk-demo".to_owned()),
                state: Some("missing".to_owned()),
            });
        cold.cache_events
            .push(jackin_diagnostics::CacheEventSummary {
                kind: "image_cache_miss".to_owned(),
                stage: Some("derived image".to_owned()),
                message: "missing".to_owned(),
                detail: Some("missing_local_image".to_owned()),
            });
        cold.cache_events
            .push(jackin_diagnostics::CacheEventSummary {
                kind: "image_cache_hit".to_owned(),
                stage: Some("derived image".to_owned()),
                message: "sibling reused".to_owned(),
                detail: Some("recipe_hash_match".to_owned()),
            });
        cold.docker_build_steps
            .push(jackin_diagnostics::DockerBuildStepSummary {
                step: "#46".to_owned(),
                label: "exporting to image".to_owned(),
                duration_ms: Some(76_500),
                cached: false,
            });
        cold.skipped_timings
            .push(jackin_diagnostics::SkippedTimingSummary {
                stage: "credentials".to_owned(),
                name: "manifest_env".to_owned(),
                detail: "no manifest env entries".to_owned(),
            });
        let mut warm = summary_with_startup(900);
        warm.prewarmed_dind_adoptions
            .push(jackin_diagnostics::PrewarmedDindAdoptionSummary {
                outcome: "adopted".to_owned(),
                detail: Some(
                    "ready_ms=7;source=state;state_age_ms=12;prewarm_ready_ms=34".to_owned(),
                ),
                reason: None,
                source: Some("state".to_owned()),
                ready_ms: Some(7),
                prewarm_ready_ms: Some(34),
                state_age_ms: Some(12),
            });
        let runs = vec![
            (PathBuf::from("cold.jsonl"), cold),
            (PathBuf::from("warm.jsonl"), warm),
        ];

        let json = comparison_json(&runs, DiagnosticsCompareBaseline::Fastest, &[]);

        assert_eq!(json["baseline"], "fastest");
        assert_eq!(json["startup_baseline_ms"], 900);
        assert_eq!(json["fastest_startup_run"]["label"], "warm");
        assert_eq!(json["fastest_startup_run"]["startup_ms"], 900);
        assert_eq!(json["slowest_startup_run"]["run_id"], "jk-run-cold");
        assert_eq!(json["slowest_startup_run"]["startup_ms"], 5_000);
        assert_eq!(json["startup_spread_ms"], 4_100);
        assert_eq!(json["selected_plan_counts"]["BuildAndCreate"], 1);
        assert_eq!(json["selected_plan_counts"]["none"], 1);
        assert_eq!(json["cache_decision_counts"]["image_cache_miss"], 1);
        assert_eq!(json["cache_decision_counts"]["none"], 1);
        assert_eq!(json["prewarmed_dind_adoption_counts"]["adopted"], 1);
        assert_eq!(json["prewarmed_dind_adoption_counts"]["none"], 1);
        assert_eq!(json["slowest_stage_ms"]["name"], "derived image");
        assert_eq!(json["slowest_stage_ms"]["label"], "jk-run-cold");
        assert_eq!(json["slowest_timing_ms"]["name"], "image/docker_build");
        assert_eq!(
            json["slowest_docker_build_step_ms"]["name"],
            "#46 exporting to image"
        );
        assert_eq!(json["runs"][0]["run_id"], "jk-run-cold");
        assert_eq!(json["runs"][0]["startup_ms"], 5_000);
        assert_eq!(json["runs"][0]["timeline_ms"], 6_000);
        assert_eq!(json["runs"][0]["startup_delta"], "+4.1s, 5.6x slower");
        assert_eq!(json["runs"][0]["startup_delta_ms"], 4_100);
        assert_eq!(json["runs"][0]["startup_saved_ms"], -4_100);
        assert_eq!(json["runs"][0]["startup_ratio"], 5_000.0 / 900.0);
        assert_eq!(json["runs"][0]["cache_misses"], 1);
        assert_eq!(json["runs"][0]["selected_plan"], "BuildAndCreate");
        assert_eq!(json["runs"][0]["selected_reason"], "missing_local_image");
        assert_eq!(
            json["runs"][0]["launch_plan_events"][0]["kind"],
            "launch_plan"
        );
        assert_eq!(json["runs"][0]["launch_plan_events"][0]["state"], "missing");
        assert_eq!(
            json["runs"][0]["build_context_snapshots"][0]["source"],
            "workspace"
        );
        assert_eq!(json["runs"][0]["build_context_snapshots"][0]["files"], 7);
        assert_eq!(
            json["runs"][0]["image_build_sources"][0]["source"],
            "workspace_dockerfile"
        );
        assert_eq!(
            json["runs"][0]["image_build_sources"][0]["pull_base_image"],
            false
        );
        assert_eq!(
            json["runs"][0]["build_context_snapshots"][0]["context_dir"],
            "/tmp/context"
        );
        assert_eq!(json["runs"][0]["max_build_context_bytes"], 2048);
        assert_eq!(
            json["runs"][0]["stage_durations_ms"]["derived image"][0],
            2_000
        );
        assert_eq!(
            json["runs"][0]["timing_durations_ms"]["image/docker_build"][0],
            1_500
        );
        assert_eq!(json["runs"][0]["slowest_stage_ms"]["name"], "derived image");
        assert_eq!(json["runs"][0]["slowest_timing_ms"]["duration_ms"], 1_500);
        assert_eq!(
            json["runs"][0]["slowest_docker_build_step_ms"]["name"],
            "#46 exporting to image"
        );
        assert_eq!(
            json["runs"][0]["slowest_docker_build_step_ms"]["duration_ms"],
            76_500
        );
        assert_eq!(json["runs"][0]["docker_build_steps"][0]["step"], "#46");
        assert_eq!(
            json["runs"][0]["docker_build_steps"][0]["name"],
            "#46 exporting to image"
        );
        assert_eq!(
            json["runs"][0]["docker_build_steps"][0]["duration_ms"],
            76_500
        );
        assert_eq!(
            json["runs"][0]["cache_decision"]["detail"],
            "missing_local_image"
        );
        assert_eq!(
            json["runs"][0]["cache_decisions"].as_array().unwrap().len(),
            2
        );
        assert_eq!(
            json["runs"][0]["cache_decisions"][1]["detail"],
            "recipe_hash_match"
        );
        assert_eq!(
            json["runs"][0]["skipped_timings"][0]["name"],
            "manifest_env"
        );
        assert_eq!(
            json["runs"][1]["prewarmed_dind_adoptions"][0]["outcome"],
            "adopted"
        );
        assert_eq!(
            json["runs"][1]["prewarmed_dind_adoptions"][0]["detail"],
            "ready_ms=7;source=state;state_age_ms=12;prewarm_ready_ms=34"
        );
        assert_eq!(
            json["runs"][1]["prewarmed_dind_adoptions"][0]["source"],
            "state"
        );
        assert_eq!(
            json["runs"][1]["prewarmed_dind_adoptions"][0]["ready_ms"],
            7
        );
        assert_eq!(
            json["runs"][1]["prewarmed_dind_adoptions"][0]["prewarm_ready_ms"],
            34
        );
        assert_eq!(
            json["runs"][1]["prewarmed_dind_adoptions"][0]["state_age_ms"],
            12
        );
    }

    #[test]
    fn compare_output_requires_json_format() {
        let args = DiagnosticsCompareArgs {
            runs: vec!["cold".to_owned(), "warm".to_owned()],
            top: 10,
            baseline: DiagnosticsCompareBaseline::Fastest,
            format: DiagnosticsCompareFormat::Text,
            output: Some(PathBuf::from("compare.json")),
            labels: Vec::new(),
        };

        let error = validate_compare_args(&args).unwrap_err();

        assert_eq!(error.to_string(), "--output requires --format json");
    }

    #[test]
    fn compare_labels_must_match_run_count() {
        let args = DiagnosticsCompareArgs {
            runs: vec!["cold".to_owned(), "warm".to_owned()],
            top: 10,
            baseline: DiagnosticsCompareBaseline::Fastest,
            format: DiagnosticsCompareFormat::Json,
            output: None,
            labels: vec!["cold".to_owned()],
        };

        let error = validate_compare_args(&args).unwrap_err();

        assert_eq!(
            error.to_string(),
            "--label must be supplied once per run when used"
        );
    }

    #[test]
    fn comparison_json_uses_explicit_labels() {
        let mut cold = summary_with_startup(5_000);
        cold.run_id = Some("jk-run-cold".to_owned());
        let runs = vec![
            (PathBuf::from("a.jsonl"), cold),
            (PathBuf::from("b.jsonl"), summary_with_startup(900)),
        ];
        let labels = vec!["cold-before".to_owned(), "warm-after".to_owned()];

        let json = comparison_json(&runs, DiagnosticsCompareBaseline::Fastest, &labels);

        assert_eq!(json["runs"][0]["label"], "cold-before");
        assert_eq!(json["runs"][1]["label"], "warm-after");
        assert_eq!(json["fastest_startup_run"]["label"], "warm-after");
        assert_eq!(json["slowest_startup_run"]["label"], "cold-before");
    }

    #[test]
    fn compare_output_writes_json_artifact_with_trailing_newline() {
        let runs = vec![
            (PathBuf::from("cold.jsonl"), summary_with_startup(5_000)),
            (PathBuf::from("warm.jsonl"), summary_with_startup(900)),
        ];
        let output =
            render_comparison_json(&runs, DiagnosticsCompareBaseline::Fastest, &[]).unwrap();
        let path = std::env::temp_dir().join(format!(
            "jackin-diagnostics-compare-{}-{}.json",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));

        write_compare_output(&path, &output).unwrap();
        let written = fs::read_to_string(&path).unwrap();
        drop(fs::remove_file(&path));

        assert!(written.ends_with('\n'));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&written).unwrap()["baseline"],
            "fastest"
        );
    }

    #[test]
    fn byte_formatter_uses_binary_units() {
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(2048), "2.0 KiB");
        assert_eq!(format_bytes(3 * 1024 * 1024), "3.0 MiB");
    }

    #[test]
    fn comparison_names_are_ranked_by_slowest_observed_duration() {
        let runs = vec![
            (
                PathBuf::from("first.jsonl"),
                summary_with_stages([("credentials", 200), ("role", 10)]),
            ),
            (
                PathBuf::from("second.jsonl"),
                summary_with_stages([("derived image", 500), ("credentials", 40)]),
            ),
        ];

        let names = comparison_names(&runs, 2, |summary| &summary.stage_durations_ms);

        assert_eq!(names, vec!["derived image", "credentials"]);
    }

    #[test]
    fn comparison_names_are_truncated_to_display_width() {
        assert_eq!(truncate_name("short", 10), "short");
        assert_eq!(truncate_name("abcdefghijklmnopqrstuvwxyz", 8), "abcdefg…");
    }

    #[test]
    fn build_context_comparison_uses_max_snapshot_per_run() {
        let mut summary = summary_with_stages([]);
        summary
            .build_context_snapshots
            .push(jackin_diagnostics::BuildContextSnapshotSummary {
                source: Some("workspace".to_owned()),
                files: 2,
                bytes: 1024,
                context_dir: Some("/tmp/one".to_owned()),
            });
        summary
            .build_context_snapshots
            .push(jackin_diagnostics::BuildContextSnapshotSummary {
                source: Some("published".to_owned()),
                files: 5,
                bytes: 512,
                context_dir: Some("/tmp/two".to_owned()),
            });

        assert_eq!(max_build_context_bytes(&summary), Some(1024));
        assert_eq!(max_build_context_files(&summary), Some(5));
    }

    #[test]
    fn cache_comparison_uses_first_cache_decision_per_run() {
        let mut summary = summary_with_stages([]);
        summary
            .cache_events
            .push(jackin_diagnostics::CacheEventSummary {
                kind: "image_cache_miss".to_owned(),
                stage: Some("derived image".to_owned()),
                message: "rebuild".to_owned(),
                detail: Some("hooks_hash_changed".to_owned()),
            });

        assert_eq!(summary.cache_events[0].kind, "image_cache_miss");
        assert_eq!(
            summary.cache_events[0].detail.as_deref(),
            Some("hooks_hash_changed")
        );
    }

    #[test]
    fn skipped_timing_comparison_lists_union_across_runs() {
        let mut first = summary_with_stages([]);
        first
            .skipped_timings
            .push(jackin_diagnostics::SkippedTimingSummary {
                stage: "credentials".to_owned(),
                name: "operator_env".to_owned(),
                detail: "skipped".to_owned(),
            });
        let mut second = summary_with_stages([]);
        second
            .skipped_timings
            .push(jackin_diagnostics::SkippedTimingSummary {
                stage: "credentials".to_owned(),
                name: "manifest_env".to_owned(),
                detail: "skipped".to_owned(),
            });
        let runs = vec![
            (PathBuf::from("warm.jsonl"), first.clone()),
            (PathBuf::from("attach.jsonl"), second),
        ];

        assert_eq!(
            skipped_timing_names(&runs, 10),
            vec![
                "credentials/manifest_env".to_owned(),
                "credentials/operator_env".to_owned()
            ]
        );
        assert_eq!(
            skipped_timing_detail(&first, "credentials/operator_env"),
            Some("skipped")
        );
        assert_eq!(
            skipped_timing_detail(&first, "credentials/manifest_env"),
            None
        );
    }

    #[test]
    fn launch_plan_comparison_uses_selected_plan() {
        let mut summary = summary_with_stages([]);
        summary
            .launch_plan_events
            .push(jackin_diagnostics::LaunchPlanEventSummary {
                kind: "launch_plan_rejected".to_owned(),
                plan: Some("AttachExisting".to_owned()),
                reason: Some("container_missing".to_owned()),
                container: Some("jk-demo".to_owned()),
                state: Some("missing".to_owned()),
            });
        summary
            .launch_plan_events
            .push(jackin_diagnostics::LaunchPlanEventSummary {
                kind: "launch_plan".to_owned(),
                plan: Some("CreateFromValidImage".to_owned()),
                reason: Some("recipe_hash_match".to_owned()),
                container: Some("jk-demo".to_owned()),
                state: Some("missing".to_owned()),
            });

        let selected = selected_launch_plan(&summary).unwrap();

        assert_eq!(selected.plan.as_deref(), Some("CreateFromValidImage"));
        assert_eq!(selected.reason.as_deref(), Some("recipe_hash_match"));
    }

    #[test]
    fn prewarmed_dind_comparison_uses_latest_adoption() {
        let mut summary = summary_with_stages([]);
        summary
            .prewarmed_dind_adoptions
            .push(jackin_diagnostics::PrewarmedDindAdoptionSummary {
                outcome: "skipped".to_owned(),
                detail: Some("locked".to_owned()),
                reason: Some("locked".to_owned()),
                source: None,
                ready_ms: None,
                prewarm_ready_ms: None,
                state_age_ms: None,
            });
        summary
            .prewarmed_dind_adoptions
            .push(jackin_diagnostics::PrewarmedDindAdoptionSummary {
                outcome: "adopted".to_owned(),
                detail: Some(
                    "ready_ms=7;source=state;state_age_ms=12;prewarm_ready_ms=34".to_owned(),
                ),
                reason: None,
                source: Some("state".to_owned()),
                ready_ms: Some(7),
                prewarm_ready_ms: Some(34),
                state_age_ms: Some(12),
            });

        let latest = last_prewarmed_dind_adoption(&summary).unwrap();

        assert_eq!(latest.outcome, "adopted");
        assert_eq!(
            super::format_prewarmed_dind_adoption_detail(latest),
            "source=state ready_ms=7 prewarm_ready_ms=34 state_age_ms=12"
        );
    }

    #[test]
    fn docker_build_step_comparison_uses_slowest_step_per_run() {
        let mut first = summary_with_stages([]);
        first
            .docker_build_steps
            .push(jackin_diagnostics::DockerBuildStepSummary {
                step: "#12".to_owned(),
                label: "RUN claude install".to_owned(),
                duration_ms: Some(1_200),
                cached: false,
            });
        first
            .docker_build_steps
            .push(jackin_diagnostics::DockerBuildStepSummary {
                step: "#12".to_owned(),
                label: "RUN claude install".to_owned(),
                duration_ms: Some(800),
                cached: true,
            });
        let mut second = summary_with_stages([]);
        second
            .docker_build_steps
            .push(jackin_diagnostics::DockerBuildStepSummary {
                step: "#46".to_owned(),
                label: "exporting to image".to_owned(),
                duration_ms: Some(76_500),
                cached: false,
            });
        let runs = vec![
            (PathBuf::from("first.jsonl"), first.clone()),
            (PathBuf::from("second.jsonl"), second),
        ];

        assert_eq!(
            docker_build_step_names(&runs, 2),
            vec![
                "#46 exporting to image".to_owned(),
                "#12 RUN claude install".to_owned()
            ]
        );
        assert_eq!(
            max_docker_build_step_duration(&first, "#12 RUN claude install"),
            Some(1_200)
        );
    }

    fn summary_with_stages<const N: usize>(
        stages: [(&str, u64); N],
    ) -> jackin_diagnostics::DiagnosticsSummary {
        let mut stage_durations_ms = BTreeMap::new();
        for (name, duration) in stages {
            stage_durations_ms.insert(name.to_owned(), vec![duration]);
        }
        jackin_diagnostics::DiagnosticsSummary {
            run_id: None,
            event_count: 0,
            event_counts: BTreeMap::new(),
            first_ts_ms: None,
            last_ts_ms: None,
            hardline_ts_ms: None,
            stage_durations_ms,
            timing_durations_ms: BTreeMap::new(),
            build_context_snapshots: Vec::new(),
            image_build_sources: Vec::new(),
            docker_build_steps: Vec::new(),
            cache_events: Vec::new(),
            launch_plan_events: Vec::new(),
            prewarmed_dind_adoptions: Vec::new(),
            skipped_timings: Vec::new(),
        }
    }

    fn summary_with_startup(startup_ms: u128) -> jackin_diagnostics::DiagnosticsSummary {
        let mut summary = summary_with_stages([]);
        summary.first_ts_ms = Some(100);
        summary.hardline_ts_ms = Some(100 + startup_ms);
        summary.last_ts_ms = Some(100 + startup_ms + 1_000);
        summary
    }
}
