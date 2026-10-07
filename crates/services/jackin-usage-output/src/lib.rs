//! jackin-usage-output: plain stdout/stderr writers for capsule CLI output.
//!
//! **Architecture Invariant:** T0.
//! Entry point: [`stdout_line`] — locked stdout line writer.

use std::fmt::Arguments;
use std::io::Write as _;

pub fn stdout_line(args: Arguments<'_>) {
    let mut stdout = std::io::stdout().lock();
    drop(writeln!(stdout, "{args}"));
}

pub fn stdout_empty_line() {
    let mut stdout = std::io::stdout().lock();
    drop(writeln!(stdout));
}

pub fn stdout_fragment(args: Arguments<'_>) {
    let mut stdout = std::io::stdout().lock();
    drop(write!(stdout, "{args}"));
}

pub fn stderr_line(args: Arguments<'_>) {
    let mut stderr = std::io::stderr().lock();
    drop(writeln!(stderr, "{args}"));
}

#[cfg(test)]
mod tests;
