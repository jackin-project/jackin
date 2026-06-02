//! Launch terminal capability checks.

use std::io::IsTerminal;

#[must_use]
pub fn rich_terminal_supported() -> bool {
    terminal_supports_rich_surface(true)
}

/// Bail with the canonical rich-terminal requirement message unless the
/// current terminal can host the launch surface.
pub fn require_rich_terminal() -> anyhow::Result<()> {
    if !rich_terminal_supported() {
        anyhow::bail!(
            "jackin load requires a rich terminal: stdin/stdout/stderr must be TTYs, TERM must not be dumb, CI must be unset, and the terminal must be at least 80x24"
        );
    }
    Ok(())
}

#[must_use]
pub fn terminal_supports_rich_surface(require_stderr: bool) -> bool {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return false;
    }
    if require_stderr && !std::io::stderr().is_terminal() {
        return false;
    }
    if std::env::var_os("CI").is_some() {
        return false;
    }
    if std::env::var("TERM").is_ok_and(|term| term == "dumb") {
        return false;
    }
    crossterm::terminal::size().is_ok_and(|(cols, rows)| cols >= 80 && rows >= 24)
}
