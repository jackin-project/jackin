//! CLI support for inspecting run diagnostics artifacts.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};

use crate::paths::JackinPaths;

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
    let runs = args
        .runs
        .iter()
        .map(|run| {
            let path = resolve_run_path(paths, run);
            let summary = jackin_diagnostics::summarize_run_file(&path)?;
            Ok((path, summary))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    print_comparison(&runs, args.top);
    Ok(())
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
    println!(
        "Cache: {} hit(s), {} miss(es)",
        summary.cache_hits(),
        summary.cache_misses()
    );

    print_duration_section("Stages", stage_rows(&summary.stage_durations_ms), top);
    print_duration_section("Timings", stage_rows(&summary.timing_durations_ms), top);
    print_launch_plan_section(summary, top);
    print_build_context_section(summary, top);
    print_build_section(summary, top);
    print_cache_section(summary, top);
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

fn print_comparison(runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)], top: usize) {
    println!("Runs");
    for (index, (path, summary)) in runs.iter().enumerate() {
        let label = comparison_label(index, path, summary);
        let timeline = summary
            .wall_duration_ms()
            .map_or_else(|| "(unknown)".to_owned(), format_duration);
        println!(
            "  {label}: {timeline}, {} event(s), {} cache hit(s), {} cache miss(es)",
            summary.event_count,
            summary.cache_hits(),
            summary.cache_misses(),
        );
    }

    print_comparison_section("Stage Comparison", runs, top, |summary| {
        &summary.stage_durations_ms
    });
    print_comparison_section("Timing Comparison", runs, top, |summary| {
        &summary.timing_durations_ms
    });
    print_launch_plan_comparison(runs);
    print_build_context_comparison(runs);
    print_cache_comparison(runs);
}

fn print_comparison_section(
    title: &str,
    runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)],
    top: usize,
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
        print!(" {:>10}", comparison_label(index, path, summary));
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

fn print_build_context_comparison(runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)]) {
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
        print!(" {:>10}", comparison_label(index, path, summary));
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

fn print_launch_plan_comparison(runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)]) {
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
        let label = comparison_label(index, path, summary);
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

fn print_cache_comparison(runs: &[(PathBuf, jackin_diagnostics::DiagnosticsSummary)]) {
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
        let label = comparison_label(index, path, summary);
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
        println!(
            "  {:>10}  {:>8} file(s)  {}",
            format_bytes(snapshot.bytes),
            snapshot.files,
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
        comparison_names, format_bytes, format_duration, max_build_context_bytes,
        max_build_context_files, resolve_run_path, selected_launch_plan, truncate_name,
    };
    use crate::paths::JackinPaths;
    use std::collections::BTreeMap;
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
                files: 2,
                bytes: 1024,
                context_dir: Some("/tmp/one".to_owned()),
            });
        summary
            .build_context_snapshots
            .push(jackin_diagnostics::BuildContextSnapshotSummary {
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
            stage_durations_ms,
            timing_durations_ms: BTreeMap::new(),
            build_context_snapshots: Vec::new(),
            docker_build_steps: Vec::new(),
            cache_events: Vec::new(),
            launch_plan_events: Vec::new(),
        }
    }
}
