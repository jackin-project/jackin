//! The verified native generator's exact actionlint syntax bridge.

use std::{ffi::OsString, path::Path};

use anyhow::{Result, bail};

// Match Velnor's workflow_lint_args; keep expression and ShellCheck validation active.
const COMPATIBILITY_ARGS: [&str; 5] = [
    "-color",
    "-ignore",
    r#"^unexpected key "background" for step to (run shell command|execute action)\. "#,
    "-ignore",
    r#"^step must run script with "run" section or run action with "uses" section$"#,
];

pub(super) fn actionlint_args(root: &Path) -> Result<Vec<OsString>> {
    let workflows = root.join(".github/workflows");
    if !workflows.is_dir() {
        bail!("{} does not exist", workflows.display());
    }
    let mut files = Vec::new();
    for entry in crate::fs_util::read_dir_sorted(&workflows)? {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "yml") {
            files.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .as_os_str()
                    .to_owned(),
            );
        }
    }
    if files.is_empty() {
        bail!("no workflow files found under {}", workflows.display());
    }
    files.sort();
    let mut args = COMPATIBILITY_ARGS
        .iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    args.extend(files);
    Ok(args)
}
