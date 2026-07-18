//! Failure-context helpers for captured process output.

use std::path::Path;

pub(super) fn e2e_failure_context(home: &Path, stdout: &str, stderr: &str) -> String {
    let mut out = String::new();
    out.push_str(&diagnostics_snapshot(home));
    out.push_str("\nstdout tail:\n");
    out.push_str(&tail_text(stdout));
    out.push_str("\nstderr tail:\n");
    out.push_str(&tail_text(stderr));
    out
}

pub(super) fn diagnostics_snapshot(home: &Path) -> String {
    let artifact_dir = home.join(".jackin/data").join("diagnostics").join("runs");
    format!(
        "captured process output is the diagnostic source; legacy artifact directory exists={} ({})",
        artifact_dir.exists(),
        artifact_dir.display()
    )
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
