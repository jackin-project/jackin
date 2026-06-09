//! Launch terminal capability checks.

use std::io::IsTerminal;

use ratatui::layout::Rect;

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

#[must_use]
pub fn current_terminal_area() -> Rect {
    terminal_area_from_size(crossterm::terminal::size().ok())
}

#[must_use]
pub fn terminal_area_from_size(size: Option<(u16, u16)>) -> Rect {
    size.map_or_else(Rect::default, |(width, height)| {
        Rect::new(0, 0, width, height)
    })
}

#[cfg(test)]
mod tests;
