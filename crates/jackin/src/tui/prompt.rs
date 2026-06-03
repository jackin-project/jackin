//! Interactive terminal prompts: yes/no confirmation and single-item selection used by CLI flows.
//!
//! Invariant: callers must check `require_interactive_stdin` (or call it themselves)
//! before invoking any prompt — all prompts bail if stdin is not a TTY.
//!
//! Not responsible for: ratatui-based TUI dialogs or non-interactive output.

use owo_colors::OwoColorize;
use std::io::{self, Write};

use jackin_diagnostics::{is_debug_mode, rich_terminal_owned};

const fn rgb(c: (u8, u8, u8)) -> owo_colors::Rgb {
    owo_colors::Rgb(c.0, c.1, c.2)
}

const PHOSPHOR_GREEN: (u8, u8, u8) = (0, 255, 65);
const PHOSPHOR_DIM: (u8, u8, u8) = (0, 140, 30);

// ── Interactive prompt ───────────────────────────────────────────────────

/// Bail with `msg` when stdin is not an interactive terminal.
///
/// Call at the top of any flow that would otherwise prompt the operator via
/// `dialoguer` or `prompt_choice`. Shared across CLI call sites (workspace
/// edit/prune, sensitive-mount confirmation) so the non-TTY guard pattern
/// exists in one place instead of being copy-pasted with drifting messages.
///
/// Returns `Ok(())` when stdin is a terminal so the caller can continue
/// into its prompt. Does NOT prompt itself.
pub fn require_interactive_stdin(msg: &str) -> anyhow::Result<()> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("{msg}");
    }
    Ok(())
}

/// Display a numbered prompt on stderr and read a choice from stdin.
/// Returns the 0-based index of the chosen option.
/// Errors if stdin is not a terminal.
pub fn prompt_choice(message: &str, options: &[&str]) -> anyhow::Result<usize> {
    require_interactive_stdin(
        "ambiguous target requires interactive input, but stdin is not a terminal",
    )?;
    prompt_choice_from(
        message,
        options,
        &mut std::io::stdin().lock(),
        &mut io::stderr(),
    )
}

fn prompt_choice_from<R: io::BufRead, W: Write>(
    message: &str,
    options: &[&str],
    input: &mut R,
    output: &mut W,
) -> anyhow::Result<usize> {
    if options.is_empty() {
        anyhow::bail!("prompt_choice requires at least one option");
    }

    writeln!(output, "{message}")?;
    for (i, option) in options.iter().enumerate() {
        writeln!(output, "  [{}] {}", i + 1, option)?;
    }

    loop {
        write!(output, "Choose [1/{}]: ", options.len())?;
        output.flush()?;

        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            anyhow::bail!("input closed before a choice was made");
        }
        let trimmed = line.trim();
        if let Some(index) = trimmed.parse::<usize>().ok().and_then(|n| {
            if n >= 1 && n <= options.len() {
                Some(n - 1)
            } else {
                None
            }
        }) {
            return Ok(index);
        }

        writeln!(
            output,
            "Invalid choice {trimmed:?}; enter a number from 1 to {}.",
            options.len()
        )?;
    }
}

/// Display a spinner while waiting, returning when `poll` returns `Ok(())`.
///
/// `poll` is called up to `max_attempts` times with `interval` between calls.
/// The spinner animates smoothly independent of the poll interval.
pub async fn spin_wait<F, Fut>(
    message: &str,
    max_attempts: u32,
    interval: std::time::Duration,
    mut poll: F,
) -> anyhow::Result<()>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<()>>,
{
    const FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    const SPIN_MS: u64 = 80;
    let mg = rgb(PHOSPHOR_GREEN);
    let mut last_err = None;
    let mut frame_idx: usize = 0;

    let debug = is_debug_mode();
    // A full-screen rich surface (the launch cockpit) owns the terminal —
    // its own waiting animation conveys progress, so the spinner must stay
    // silent or it streams over the alternate screen.
    let suppressed = rich_terminal_owned();
    for _attempt in 0..max_attempts {
        if debug && !suppressed {
            eprint!("\r\x1b[2K");
            let _ = io::stderr().flush();
        }
        match poll().await {
            Ok(()) => {
                if !suppressed {
                    eprint!("\r\x1b[2K");
                    let _ = io::stderr().flush();
                }
                return Ok(());
            }
            Err(e) => last_err = Some(e),
        }
        let spins = interval.as_millis() as u64 / SPIN_MS;
        for _ in 0..spins {
            if !suppressed {
                let frame = FRAMES[frame_idx % FRAMES.len()];
                eprint!(
                    "\r   {}   {}",
                    frame.color(mg).bold(),
                    message.color(rgb(PHOSPHOR_DIM)).bold()
                );
                let _ = io::stderr().flush();
            }
            tokio::time::sleep(std::time::Duration::from_millis(SPIN_MS)).await;
            frame_idx += 1;
        }
    }
    if !suppressed {
        eprint!("\r\x1b[2K");
        let _ = io::stderr().flush();
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("timed out: {message}")))
}

#[cfg(test)]
mod tests;
