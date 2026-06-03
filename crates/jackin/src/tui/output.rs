//! Formatted terminal output helpers: `tprintln!` macro and status-line writing.
//!
//! Not responsible for: spinner animation or interactive prompts —
//! those live in `src/tui/animation.rs` and `src/tui/prompt.rs`.

use owo_colors::OwoColorize;
use std::io::{self, Write};

use super::{PHOSPHOR_GREEN, rgb};

// ── Color palette ────────────────────────────────────────────────────────

const ROSE: (u8, u8, u8) = (210, 100, 100);

pub fn step_fail(msg: &str) {
    eprintln!("       {}", msg.color(rgb(ROSE)));
}

// ── Deploying message ────────────────────────────────────────────────────

pub async fn print_deploying(role_name: &str) {
    eprintln!();
    eprintln!(
        "  {}",
        format!("Deploying {role_name} into an isolated container...")
            .color(rgb(PHOSPHOR_GREEN))
            .bold()
    );
    eprintln!();

    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    clear_screen();
}

// ── Utility ──────────────────────────────────────────────────────────────

pub fn fatal(msg: &str) {
    eprintln!();
    let mut lines = msg.lines();
    let first = lines.next().unwrap_or("(no error message)");
    eprintln!(
        "  {} {}",
        "error:".color(rgb(ROSE)),
        first.color(rgb(ROSE)).bold(),
    );
    for line in lines {
        eprintln!("{line}");
    }
}

pub fn clear_screen() {
    eprint!("\x1b[2J\x1b[H");
    let _ = io::stderr().flush();
}

/// Print a hint line with a highlighted command.
pub fn hint(prefix: &str, command: &str, suffix: &str) {
    println!(
        "{prefix}{}{suffix}",
        command.color(rgb(PHOSPHOR_GREEN)).bold(),
    );
}
