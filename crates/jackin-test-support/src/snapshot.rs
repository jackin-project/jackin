//! Shared snapshot string helpers for insta / TUI characterization tests.
//!
//! Keeps redaction and line-normalization in one place so console, capsule,
//! and host test suites do not re-invent volatile-token scrubbing (residual
//! R-snapshot-helpers).

#[cfg(test)]
#[path = "snapshot/tests.rs"]
mod tests;

/// Replace sequences of digits that look like epoch ms / large counters with
/// a stable placeholder so snapshot diffs ignore clock noise.
#[must_use]
pub fn redact_digit_runs(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            let mut run = String::new();
            run.push(c);
            while chars.peek().is_some_and(|n| n.is_ascii_digit()) {
                run.push(chars.next().expect("peeked digit"));
            }
            // Short numbers (ports, counts < 4 digits) stay; long runs are volatile.
            if run.len() >= 4 {
                out.push_str("<digits>");
            } else {
                out.push_str(&run);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Normalize trailing whitespace per line and strip trailing blank lines —
/// common when capturing ratatui buffers into strings for insta.
#[must_use]
pub fn normalize_snapshot_text(input: &str) -> String {
    let lines: Vec<String> = input
        .lines()
        .map(|line| line.trim_end().to_owned())
        .collect();
    let mut end = lines.len();
    while end > 0 && lines[end - 1].is_empty() {
        end -= 1;
    }
    let mut out = lines[..end].join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}
