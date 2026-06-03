//! Formatted terminal output helpers for launch surfaces.
//!
//! These helpers write status and decorative lines to stderr so the operator
//! sees load progress and failure messages alongside the launch cockpit output.
//! They are intentionally simple — no ratatui widgets, no raw-mode management.

use owo_colors::OwoColorize as _;

use crate::{PHOSPHOR_GREEN, Rgb};

const ROSE: Rgb = Rgb::new(210, 100, 100);

fn owo_rgb(rgb: Rgb) -> owo_colors::Rgb {
    owo_colors::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Print a dimmed red error step line to stderr.
pub fn step_fail(msg: &str) {
    eprintln!("       {}", msg.color(owo_rgb(ROSE)));
}

/// Clear the terminal screen via ANSI escape codes.
pub fn clear_screen() {
    eprint!("\x1b[2J\x1b[H");
    let _ = std::io::Write::flush(&mut std::io::stderr());
}

/// Print a hint line with a highlighted command to stdout.
pub fn hint(prefix: &str, command: &str, suffix: &str) {
    println!(
        "{prefix}{}{suffix}",
        command.color(owo_rgb(PHOSPHOR_GREEN)).bold(),
    );
}

/// Print a fatal error to stderr.
pub fn fatal(msg: &str) {
    eprintln!();
    let mut lines = msg.lines();
    let first = lines.next().unwrap_or("(no error message)");
    eprintln!(
        "  {} {}",
        "error:".color(owo_rgb(ROSE)),
        first.color(owo_rgb(ROSE)).bold(),
    );
    for line in lines {
        eprintln!("{line}");
    }
}

/// Animate a "deploying" banner then clear the screen.
pub async fn print_deploying(role_name: &str) {
    eprintln!();
    eprintln!(
        "  {}",
        format!("Deploying {role_name} into an isolated container...")
            .color(owo_rgb(PHOSPHOR_GREEN))
            .bold()
    );
    eprintln!();

    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    clear_screen();
}
