//! Failure-context helpers for captured process output.

use std::path::Path;

pub(super) fn e2e_failure_context(home: &Path, stdout: &str, stderr: &str) -> String {
    let mut out = String::new();
    out.push_str(&diagnostics_snapshot(home));
    out.push_str("\nstdout excerpt:\n");
    out.push_str(&transcript_excerpt(stdout));
    out.push_str("\nstderr excerpt:\n");
    out.push_str(&transcript_excerpt(stderr));
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

/// Keep startup context, the first explicit error, and the final output.
/// Terminal animations may produce megabytes without a newline, so line
/// counts cannot bound this diagnostic. Slice only at UTF-8 boundaries.
pub(super) fn transcript_excerpt(contents: &str) -> String {
    const SECTION_BYTES: usize = 16 * 1024;
    if contents.len() <= SECTION_BYTES * 2 {
        return contents.to_owned();
    }
    let head_end = contents.floor_char_boundary(SECTION_BYTES);
    let tail_start = contents.ceil_char_boundary(contents.len() - SECTION_BYTES);
    let mut excerpt = contents[..head_end].to_owned();
    let error_start = ["Error:", "error:", "panicked at"]
        .into_iter()
        .filter_map(|marker| contents.find(marker))
        .min();
    if let Some(error_start) = error_start
        && (head_end..tail_start).contains(&error_start)
    {
        let error_end = contents.floor_char_boundary((error_start + SECTION_BYTES).min(tail_start));
        excerpt.push_str("\n... first error excerpt ...\n");
        excerpt.push_str(&contents[error_start..error_end]);
    }
    excerpt.push_str("\n... transcript truncated; final output follows ...\n");
    excerpt.push_str(&contents[tail_start..]);
    excerpt
}
