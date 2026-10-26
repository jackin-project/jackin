use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args;

#[derive(Args)]
pub(crate) struct PtyFixtureArgs {
    capture: PathBuf,
    out_bin: PathBuf,
}

pub(crate) fn run(args: PtyFixtureArgs) -> Result<()> {
    let raw = fs::read(&args.capture)
        .with_context(|| format!("failed to read {}", args.capture.display()))?;
    if raw.is_empty() {
        bail!("PTY fixture capture {} is empty", args.capture.display());
    }
    if let Some(parent) = args.out_bin.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&args.out_bin, &raw)
        .with_context(|| format!("failed to write {}", args.out_bin.display()))?;
    #[expect(clippy::print_stdout, reason = "xtask CLI result")]
    {
        println!(
            "wrote {} PTY bytes to {}",
            raw.len(),
            args.out_bin.display()
        );
    }
    Ok(())
}
