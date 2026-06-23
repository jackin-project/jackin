//! Formatted prune/cleanup terminal output shared by runtime and diagnostics.

use owo_colors::OwoColorize;
use std::fmt::Arguments;
use std::io::Write;

const STATUS_COLUMN: usize = 78;

fn flush_stdout() {
    drop(std::io::stdout().flush());
}

fn stdout_line(args: Arguments<'_>) {
    let mut stdout = std::io::stdout().lock();
    drop(writeln!(stdout, "{args}"));
}

fn stdout_fragment(args: Arguments<'_>) {
    let mut stdout = std::io::stdout().lock();
    drop(write!(stdout, "{args}"));
}

fn stderr_line(args: Arguments<'_>) {
    let mut stderr = std::io::stderr().lock();
    drop(writeln!(stderr, "{args}"));
}

pub fn section(label: &str, detail: impl std::fmt::Display) {
    stdout_line(format_args!(""));
    stdout_line(format_args!("  {} {}", label.bold(), detail.dimmed()));
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
    stdout_fragment(format_args!("    {} {}", prefix.bold(), dots.dimmed()));
    flush_stdout();
    PendingRow { finalized: false }
}

#[must_use]
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
    stdout_line(format_args!("    {} {detail}", "OK".green().bold()));
}

pub fn skip(detail: impl std::fmt::Display) {
    stdout_line(format_args!("    {}", "SKIP".yellow().bold()));
    stdout_line(format_args!("      {detail}"));
}

pub fn failed(detail: impl std::fmt::Display) {
    stderr_line(format_args!("    {}", "FAILED".red().bold()));
    stderr_line(format_args!("      {detail}"));
}

impl PendingRow {
    pub fn ok(mut self) {
        self.finalized = true;
        stdout_line(format_args!(" {}", "OK".green().bold()));
    }

    pub fn skip(mut self, reason: impl std::fmt::Display) {
        self.finalized = true;
        stdout_line(format_args!(" {}", "SKIP".yellow().bold()));
        stdout_line(format_args!("      {reason}"));
    }

    pub fn failed(mut self, reason: impl std::fmt::Display) {
        self.finalized = true;
        stdout_line(format_args!(" {}", "FAILED".red().bold()));
        stdout_line(format_args!("      {reason}"));
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
            stdout_line(format_args!(" {}", "FAILED".red().bold()));
            stdout_line(format_args!("      row not finalized"));
        }
    }
}

#[cfg(test)]
mod tests;
