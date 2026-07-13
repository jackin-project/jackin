#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::manual_assert,
    clippy::duration_suboptimal_units,
    clippy::filter_map_next,
    clippy::map_unwrap_or,
    clippy::redundant_closure,
    unreachable_pub,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]

//! Failure-context dump helpers used by the e2e harness when a test
//! panics: latest docker build log + diagnostics snapshot + tail of the
//! captured stdout/stderr so the failure message lands with the context
//! an operator needs to triage.

use std::path::{Path, PathBuf};

pub(super) fn e2e_failure_context(home: &Path, stdout: &str, stderr: &str) -> String {
    let mut out = String::new();
    if let Some(path) = latest_docker_build_log(home) {
        out.push_str("latest docker build log: ");
        out.push_str(&path.display().to_string());
        out.push('\n');
        match std::fs::read_to_string(&path) {
            Ok(contents) => append_tail_lines(&mut out, &contents),
            Err(error) => {
                out.push_str("failed to read docker build log: ");
                out.push_str(&error.to_string());
                out.push('\n');
            }
        }
    } else {
        out.push_str("no docker build log found\n");
    }
    out.push_str("diagnostics:\n");
    out.push_str(&diagnostics_snapshot(home));
    out.push_str("\nstdout tail:\n");
    out.push_str(&tail_text(stdout));
    out.push_str("\nstderr tail:\n");
    out.push_str(&tail_text(stderr));
    out
}

pub(super) fn diagnostics_snapshot(home: &Path) -> String {
    let dir = home.join(".jackin/data/diagnostics/runs");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return format!("no diagnostics directory at {}", dir.display());
    };
    let mut files = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            metadata
                .modified()
                .ok()
                .map(|modified| (modified, entry.path()))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(modified, _)| *modified);
    let Some((_, latest)) = files.last() else {
        return format!("no diagnostics files in {}", dir.display());
    };

    let mut out = format!("latest diagnostics: {}\n", latest.display());
    match std::fs::read_to_string(latest) {
        Ok(contents) => {
            append_tail_lines(&mut out, &contents);
        }
        Err(error) => {
            out.push_str("failed to read diagnostics file: ");
            out.push_str(&error.to_string());
            out.push('\n');
        }
    }

    let Some(stem) = latest.file_stem().and_then(|stem| stem.to_str()) else {
        return out;
    };
    let build_log = latest.with_file_name(format!("{stem}.docker-build.log"));
    if let Ok(contents) = std::fs::read_to_string(&build_log) {
        out.push_str("latest docker build log: ");
        out.push_str(&build_log.display().to_string());
        out.push('\n');
        append_tail_lines(&mut out, &contents);
    }
    out
}

pub(super) fn latest_docker_build_log(home: &Path) -> Option<PathBuf> {
    let dir = home.join(".jackin/data/diagnostics/runs");
    let mut files = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".docker-build.log"))
        })
        .filter_map(|path| {
            std::fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .ok()
                .map(|modified| (modified, path))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(modified, _)| *modified);
    files.pop().map(|(_, path)| path)
}

pub(super) fn append_tail_lines(out: &mut String, contents: &str) {
    let mut lines = std::collections::VecDeque::with_capacity(80);
    for line in contents.lines() {
        if lines.len() == 80 {
            lines.pop_front();
        }
        lines.push_back(line);
    }
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
}

pub(super) fn tail_text(contents: &str) -> String {
    let mut lines = std::collections::VecDeque::with_capacity(80);
    for line in contents.lines() {
        if lines.len() == 80 {
            lines.pop_front();
        }
        lines.push_back(line);
    }
    lines.into_iter().collect::<Vec<_>>().join("\n")
}
