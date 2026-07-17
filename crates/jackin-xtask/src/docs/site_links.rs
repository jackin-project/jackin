//! Generated documentation-site link validation.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::cmd;

pub(crate) fn run() -> Result<()> {
    let site_url = required("DOCS_SITE_URL")?;
    let workspace = PathBuf::from(required("GITHUB_WORKSPACE")?);
    let blob_url = required("JACKIN_REPO_BLOB_URL")?;
    let edit_url = required("JACKIN_REPO_EDIT_URL")?;
    cmd::run(&mut command(&site_url, &workspace, &blob_url, &edit_url))
}

fn command(site_url: &str, workspace: &Path, blob_url: &str, edit_url: &str) -> Command {
    let public = workspace.join("docs/.output/public");
    let mut command = Command::new("lychee");
    command
        .arg("--config")
        .arg("docs/lychee.toml")
        .arg("--include-fragments")
        .arg("--remap")
        .arg(format!(
            "{site_url}/(.*) file://{}/$1",
            public.display()
        ))
        .arg("--remap")
        .arg(format!("{edit_url}/(.*) file://{}/$1", workspace.display()))
        .arg("--remap")
        .arg(format!("{blob_url}/(.*) file://{}/$1", workspace.display()))
        .args([
            "--remap",
            "https://github.com/jackin-project/jackin/issues https://api.github.com/repos/jackin-project/jackin/issues",
            "--root-dir",
        ])
        .arg(public)
        .arg("docs/.output/public/**/*.html");
    command
}

fn required(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is required"))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::PathBuf;

    use super::command;

    #[test]
    fn command_preserves_all_link_remaps() {
        let command = command(
            "https://docs.example",
            &PathBuf::from("/workspace"),
            "https://github.example/blob/main",
            "https://github.example/edit/main",
        );
        let args = command
            .get_args()
            .map(OsStr::to_string_lossy)
            .collect::<Vec<_>>();

        assert!(args.contains(
            &"https://docs.example/(.*) file:///workspace/docs/.output/public/$1".into()
        ));
        assert!(
            args.contains(&"https://github.example/blob/main/(.*) file:///workspace/$1".into())
        );
        assert!(
            args.contains(&"https://github.example/edit/main/(.*) file:///workspace/$1".into())
        );
        assert_eq!(
            args.last().map(AsRef::as_ref),
            Some("docs/.output/public/**/*.html")
        );
    }
}
