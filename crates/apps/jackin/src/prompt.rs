// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::{self, Write};

pub fn require_interactive_stdin(msg: &str) -> anyhow::Result<()> {
    use std::io::IsTerminal;
    if !io::stdin().is_terminal() {
        anyhow::bail!("{msg}");
    }
    Ok(())
}

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
