//! Formatted prune/cleanup terminal output: section headers, item rows, status columns.
//!
//! Not responsible for prune logic or Docker interaction — purely terminal
//! formatting. `PendingRow` must be finalized before drop; the Drop impl
//! emits a visible error marker if the caller forgets.
use owo_colors::OwoColorize;
use std::io::Write;

const STATUS_COLUMN: usize = 78;

fn flush_stdout() {
    drop(std::io::stdout().flush());
}

pub fn section(label: &str, detail: impl std::fmt::Display) {
    println!();
    println!("  {} {}", label.bold(), detail.dimmed());
    flush_stdout();
}

/// A pending row that started but has not yet rendered its terminal status.
///
/// Drop without calling [`PendingRow::ok`], [`PendingRow::skip`],
/// [`PendingRow::failed`], or [`PendingRow::complete`] is a programming
/// error: it would leave the dotted prefix without a status word. The Drop
/// guard catches the leak by closing the row with `FAILED row not finalized`.
#[must_use = "PendingRow leaves the dotted prefix open until finalized"]
#[derive(Debug)]
pub struct PendingRow {
    finalized: bool,
}

pub fn start(action: &str, target: impl std::fmt::Display) -> PendingRow {
    let (prefix, dots) = pending_parts(action, target);
    print!("    {} {}", prefix.bold(), dots.dimmed());
    flush_stdout();
    PendingRow { finalized: false }
}

pub fn pending_parts(action: &str, target: impl std::fmt::Display) -> (String, String) {
    let (prefix, prefix_chars) = fit_prefix(format!("{action} {target}"));
    let dots = ".".repeat(STATUS_COLUMN.saturating_sub(prefix_chars).max(3));
    (prefix, dots)
}

fn fit_prefix(prefix: String) -> (String, usize) {
    let max = STATUS_COLUMN.saturating_sub(4);
    let keep = max.saturating_sub(3);
    let mut total = 0usize;
    let mut truncate_at: Option<usize> = None;
    for (idx, _) in prefix.char_indices() {
        if total == keep && truncate_at.is_none() {
            truncate_at = Some(idx);
        }
        if total > max {
            let cut = truncate_at.unwrap_or(idx);
            let mut fitted = prefix[..cut].to_string();
            fitted.push_str("...");
            return (fitted, keep + 3);
        }
        total += 1;
    }
    (prefix, total)
}

pub fn ok(detail: impl std::fmt::Display) {
    println!("    {} {detail}", "OK".green().bold());
}

pub fn skip(detail: impl std::fmt::Display) {
    println!("    {}", "SKIP".yellow().bold());
    println!("      {detail}");
}

pub fn failed(detail: impl std::fmt::Display) {
    eprintln!("    {}", "FAILED".red().bold());
    eprintln!("      {detail}");
}

impl PendingRow {
    pub fn ok(mut self) {
        self.finalized = true;
        println!(" {}", "OK".green().bold());
    }

    pub fn skip(mut self, reason: impl std::fmt::Display) {
        self.finalized = true;
        println!(" {}", "SKIP".yellow().bold());
        println!("      {reason}");
    }

    pub fn failed(mut self, reason: impl std::fmt::Display) {
        self.finalized = true;
        println!(" {}", "FAILED".red().bold());
        println!("      {reason}");
    }

    /// Finalize the row from a `Result`: print `OK` on success, `FAILED` on error.
    pub fn complete<T, E, F>(self, result: Result<T, E>, message: F) -> Result<T, E>
    where
        F: FnOnce(&E) -> String,
    {
        match result {
            Ok(value) => {
                self.ok();
                Ok(value)
            }
            Err(error) => {
                let detail = message(&error);
                self.failed(detail);
                Err(error)
            }
        }
    }
}

impl Drop for PendingRow {
    fn drop(&mut self) {
        if !self.finalized {
            println!(" {}", "FAILED".red().bold());
            println!("      row not finalized");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered_status_column(action: &str, target: &str, status: &str) -> usize {
        let (prefix, dots) = pending_parts(action, target);
        let line = format!("    {prefix} {dots} {status}");
        line.find(status).unwrap()
    }

    #[test]
    fn pending_rows_align_status_column() {
        let cases = [
            ("Finding", "managed containers", "OK"),
            ("Stopping", "jk-dawwxb7e-jackin-thearchitect", "OK"),
            ("Reading", "instance index", "OK"),
            ("Deleting", "jk-n8ngw2d2-jackin-thearchitect", "FAILED"),
            (
                "Deleting",
                "jk-extraordinarily-long-container-name-that-needs-truncation-thearchitect",
                "FAILED",
            ),
        ];

        let columns: Vec<usize> = cases
            .iter()
            .map(|(action, target, status)| rendered_status_column(action, target, status))
            .collect();

        assert!(
            columns.windows(2).all(|pair| pair[0] == pair[1]),
            "status columns must match: {columns:?}"
        );
    }

    #[test]
    fn complete_propagates_errors_after_finalizing_row() {
        let row = PendingRow { finalized: false };
        let result: Result<(), &str> = row.complete(Err("boom"), ToString::to_string);
        assert_eq!(result, Err("boom"));
    }
}
