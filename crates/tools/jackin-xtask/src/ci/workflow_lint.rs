//! The generated CI's workflow linter (jactionlint, velnor hard decision 10), run locally with the same flags.

use std::{ffi::OsString, path::Path};

use anyhow::{Result, bail};

// Velnor's generated jactionlint job: fail on every finding, hermetic (no online checks), the pinned ShellCheck,
// no pyflakes. No file arguments: jactionlint then checks the workflows, the composite actions, dependabot.yml and
// the Actions lock file, and reads the generated .github/jactionlint.yaml for its profile.
const ARGS: [&str; 4] = [
    "--strict-exit",
    "--no-online",
    "--shellcheck=shellcheck",
    "--pyflakes=",
];

pub(super) fn jactionlint_args(root: &Path) -> Result<Vec<OsString>> {
    let workflows = root.join(".github/workflows");
    if !workflows.is_dir() {
        bail!("{} does not exist", workflows.display());
    }
    let has_workflow = crate::fs_util::read_dir_sorted(&workflows)?
        .iter()
        .any(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"));
    if !has_workflow {
        bail!("no workflow files found under {}", workflows.display());
    }
    let config = root.join(".github/jactionlint.yaml");
    if !config.is_file() {
        bail!(
            "{} does not exist; regenerate .github with velnor-actions",
            config.display()
        );
    }
    Ok(ARGS.iter().map(OsString::from).collect())
}
