//! Native jactionlint command and complete GitHub Actions file inventory.

use std::{ffi::OsString, fs, path::Path};

use anyhow::{Context, Result, bail};

const CONFIG_PATH: &str = ".github/jactionlint.yaml";
const LINT_FLAGS: [&str; 5] = [
    "--strict-exit",
    "--no-online",
    "--shellcheck=shellcheck",
    "--pyflakes=",
    "--no-color",
];

pub(super) fn jactionlint_args(root: &Path) -> Result<Vec<OsString>> {
    let github = root.join(".github");
    let github_metadata = fs::symlink_metadata(&github)
        .with_context(|| format!("reading GitHub config directory {}", github.display()))?;
    if !github_metadata.file_type().is_dir() {
        bail!("{} is not a real directory", github.display());
    }
    let config = root.join(CONFIG_PATH);
    let config_metadata = fs::symlink_metadata(&config)
        .with_context(|| format!("reading jactionlint config {}", config.display()))?;
    if !config_metadata.file_type().is_file() {
        bail!(
            "jactionlint config {} is not a regular file",
            config.display()
        );
    }

    let mut files = Vec::new();
    collect_workflows(root, Path::new(".github/workflows"), &mut files)?;
    collect_composite_actions(root, Path::new(".github/actions"), &mut files)?;
    collect_dependabot(root, &mut files)?;
    files.sort();
    if files.is_empty() {
        bail!("no workflow, composite-action, or Dependabot YAML files found");
    }

    let mut args = vec![
        OsString::from("--config-file"),
        OsString::from(CONFIG_PATH),
        OsString::from("--format"),
        OsString::from("oneline"),
    ];
    args.extend(LINT_FLAGS.iter().map(OsString::from));
    args.extend(files);
    Ok(args)
}

fn collect_workflows(root: &Path, directory: &Path, files: &mut Vec<OsString>) -> Result<()> {
    collect_yaml_files(root, directory, None, false, true, files)
}

fn collect_composite_actions(
    root: &Path,
    directory: &Path,
    files: &mut Vec<OsString>,
) -> Result<()> {
    collect_yaml_files(root, directory, Some("action.y"), true, true, files)
}

fn collect_dependabot(root: &Path, files: &mut Vec<OsString>) -> Result<()> {
    collect_yaml_files(
        root,
        Path::new(".github"),
        Some("dependabot.y"),
        false,
        false,
        files,
    )
}

fn collect_yaml_files(
    root: &Path,
    relative_directory: &Path,
    required_prefix: Option<&str>,
    missing_ok: bool,
    reject_all_symlinks: bool,
    files: &mut Vec<OsString>,
) -> Result<()> {
    let directory = root.join(relative_directory);
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if missing_ok && error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("reading {}", directory.display()));
        }
    };
    if !metadata.file_type().is_dir() {
        bail!("{} is not a real directory", directory.display());
    }

    for entry in crate::fs_util::read_dir_sorted(&directory)? {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("reading file type for {}", path.display()))?;
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let matches_pattern = lint_filename_matches(filename, required_prefix);

        if file_type.is_symlink() {
            if reject_all_symlinks || matches_pattern {
                bail!(
                    "symlink is not allowed in lint inventory: {}",
                    path.display()
                );
            }
            continue;
        }
        if file_type.is_dir() && required_prefix == Some("action.y") {
            let child = relative_directory.join(entry.file_name());
            collect_yaml_files(root, &child, required_prefix, false, true, files)?;
            continue;
        }
        if !file_type.is_file() {
            if matches_pattern {
                bail!(
                    "non-regular file is not allowed in lint inventory: {}",
                    path.display()
                );
            }
            continue;
        }
        if matches_pattern {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            files.push(relative.as_os_str().to_owned());
        }
    }
    Ok(())
}

fn lint_filename_matches(filename: &str, required_prefix: Option<&str>) -> bool {
    filename.ends_with("ml")
        && required_prefix.map_or_else(
            || filename.rfind(".y").is_some(),
            |prefix| filename.starts_with(prefix),
        )
}
