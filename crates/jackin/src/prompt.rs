//! Interactive terminal prompts: yes/no confirmation and single-item selection used by CLI flows.
//!
//! Invariant: callers must check `require_interactive_stdin` (or call it themselves)
//! before invoking any prompt — all prompts bail if stdin is not a TTY.
//!
//! Not responsible for: ratatui-based TUI dialogs or non-interactive output.

use std::io::{self, Write};

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
    if !io::stdin().is_terminal() {
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
    prompt_choice_from(message, options, &mut io::stdin().lock(), &mut io::stderr())
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

#[cfg(test)]
mod tests;
