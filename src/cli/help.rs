use std::io::Write;

use anyhow::Context as _;
use clap::CommandFactory;

use super::Cli;

/// Display long-form man-page help for the given command path.
///
/// `command` is a slice of subcommand name tokens, e.g. `["config", "auth"]`.
/// An empty slice shows help for the root `jackin` command.
///
/// Display chain (first available wins):
///   1. `man <roff-tempfile>` — full roff rendering
///   2. `less -R <txt-tempfile>` or `more <txt-tempfile>` — paged plain text
///   3. Raw print to stdout
pub fn exec(command: &[String]) -> anyhow::Result<()> {
    let root = Cli::command();

    // Traverse to the requested subcommand (immutable borrow).
    let mut target: &clap::Command = &root;
    for part in command {
        target = target
            .find_subcommand(part.as_str())
            .ok_or_else(|| anyhow::anyhow!("no such subcommand: `{}`", command.join(" ")))?;
    }

    // Generate roff man page content.
    let mut roff: Vec<u8> = Vec::new();
    clap_mangen::Man::new(target.clone())
        .render(&mut roff)
        .context("failed to render man page")?;

    // Try man(1) with the roff file.
    if try_man(&roff)? {
        return Ok(());
    }

    // Fall back to plain long-help text via pager or raw stdout.
    let text = target.clone().render_long_help().to_string();
    if try_pager(text.as_bytes())? {
        return Ok(());
    }

    println!("{text}");
    Ok(())
}

/// Write `content` to a `.1` temp file and invoke `man`.
///
/// Returns `true` if `man` was found and invoked (regardless of its exit
/// code — the user quitting with `q` exits 1 but the page was shown).
/// Returns `false` if `man` is not installed.
fn try_man(content: &[u8]) -> anyhow::Result<bool> {
    let mut tmp = tempfile::Builder::new()
        .suffix(".1")
        .tempfile()
        .context("failed to create man temp file")?;
    tmp.write_all(content)
        .context("failed to write man temp file")?;

    match std::process::Command::new("man").arg(tmp.path()).status() {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).context("man failed unexpectedly"),
    }
}

/// Write `content` to a `.txt` temp file and display via `less -R` or `more`.
///
/// Returns `true` if a pager was found and invoked.
/// Returns `false` if neither `less` nor `more` is installed.
fn try_pager(content: &[u8]) -> anyhow::Result<bool> {
    let mut tmp = tempfile::Builder::new()
        .suffix(".txt")
        .tempfile()
        .context("failed to create pager temp file")?;
    tmp.write_all(content)
        .context("failed to write pager temp file")?;

    let pagers: &[(&str, &[&str])] = &[("less", &["-R"]), ("more", &[])];
    for (pager, args) in pagers {
        let mut cmd = std::process::Command::new(pager);
        cmd.args(*args).arg(tmp.path());
        match cmd.status() {
            Ok(_) => return Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).context(format!("{pager} failed unexpectedly")),
        }
    }
    Ok(false)
}
