//! CLI support for inspecting run diagnostics artifacts.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};

use crate::paths::JackinPaths;

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum DiagnosticsCommand {
    /// Summarize a diagnostics run JSONL artifact
    Summary(DiagnosticsSummaryArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct DiagnosticsSummaryArgs {
    /// Run ID (`jk-run-...`) or path to a diagnostics JSONL file.
    pub run: String,
    /// Number of slow stage/timing/build rows to print per section.
    #[arg(long, default_value_t = 10)]
    pub top: usize,
}

pub fn run(command: &DiagnosticsCommand, paths: &JackinPaths) -> anyhow::Result<()> {
    match command {
        DiagnosticsCommand::Summary(args) => summary(args, paths),
    }
}

fn summary(args: &DiagnosticsSummaryArgs, paths: &JackinPaths) -> anyhow::Result<()> {
    let path = resolve_run_path(paths, &args.run);
    let summary = jackin_diagnostics::summarize_run_file(&path)?;
    print_summary(&summary, &path, args.top);
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
    print_build_section(summary, top);
    print_cache_section(summary, top);
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

#[cfg(test)]
mod tests {
    use super::{format_duration, resolve_run_path};
    use crate::paths::JackinPaths;

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
}
