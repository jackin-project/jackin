// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Formatted terminal output helpers for launch surfaces.
//!
//! These helpers write status and decorative lines to stderr so the operator
//! sees load progress and failure messages alongside the launch cockpit output.
//! They are intentionally simple — no ratatui widgets, no raw-mode management.
use owo_colors::OwoColorize as _;
use std::fmt::Arguments;
use std::io::Write as _;

use jackin_core::{PHOSPHOR_GREEN, Rgb};

fn owo_rgb(rgb: Rgb) -> owo_colors::Rgb {
    owo_colors::Rgb(rgb.r, rgb.g, rgb.b)
}

const ROSE: Rgb = Rgb::new(210, 100, 100);

fn stdout_line(args: Arguments<'_>) {
    let mut stdout = std::io::stdout().lock();
    drop(writeln!(stdout, "{args}"));
}

fn stderr_line(args: Arguments<'_>) {
    let mut stderr = std::io::stderr().lock();
    drop(writeln!(stderr, "{args}"));
}

fn stderr_empty_line() {
    let mut stderr = std::io::stderr().lock();
    drop(writeln!(stderr));
}

fn stderr_fragment(args: Arguments<'_>) {
    let mut stderr = std::io::stderr().lock();
    drop(write!(stderr, "{args}"));
}

/// Print a dimmed red error step line to stderr.
pub fn step_fail(msg: &str) {
    stderr_line(format_args!("       {}", msg.color(owo_rgb(ROSE))));
}

/// Clear the terminal screen via ANSI escape codes.
pub fn clear_screen() {
    stderr_fragment(format_args!("\x1b[2J\x1b[H"));
    drop(std::io::Write::flush(&mut std::io::stderr()));
}

/// Print a hint line with a highlighted command to stdout.
pub fn hint(prefix: &str, command: &str, suffix: &str) {
    stdout_line(format_args!(
        "{prefix}{}{suffix}",
        command.color(owo_rgb(PHOSPHOR_GREEN)).bold(),
    ));
}

/// Print a fatal error to stderr.
pub fn fatal(msg: &str) {
    stderr_empty_line();
    let mut lines = msg.lines();
    let first = lines.next().unwrap_or("(no error message)");
    stderr_line(format_args!(
        "  {} {}",
        "error:".color(owo_rgb(ROSE)),
        first.color(owo_rgb(ROSE)).bold(),
    ));
    for line in lines {
        stderr_line(format_args!("{line}"));
    }
}

/// Animate a "deploying" banner then clear the screen.
pub async fn print_deploying(role_name: &str) {
    stderr_empty_line();
    stderr_line(format_args!(
        "  {}",
        format!("Deploying {role_name} into an isolated container...")
            .color(owo_rgb(PHOSPHOR_GREEN))
            .bold()
    ));
    stderr_empty_line();

    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    clear_screen();
}
