use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

#[path = "workflow_lint_uses.rs"]
mod workflow_lint_uses;

const CONFIG_PATH: &str = ".github/jactionlint.yaml";
const ACTIONS_LOCK_PATH: &str = ".github/workflows/actions.lock";
const LINT_FLAGS: [&str; 5] = [
    "--strict-exit",
    "--no-online",
    "--shellcheck=shellcheck",
    "--pyflakes=",
    "--no-color",
];
const SKIPPED_DIRS: [&str; 9] = [
    ".git",
    "node_modules",
    "vendor",
    "testdata",
    "fixtures",
    ".venv",
    "venv",
    "target",
    "dist",
];

pub(super) fn jactionlint_args(root: &Path) -> Result<Vec<OsString>> {
    check_repository_inputs(root)?;
    let mut args = vec![
        OsString::from("--config-file"),
        OsString::from(CONFIG_PATH),
        OsString::from("--format"),
        OsString::from("oneline"),
    ];
    args.extend(LINT_FLAGS.iter().map(OsString::from));
    Ok(args)
}

fn check_repository_inputs(root: &Path) -> Result<()> {
    let github = root.join(".github");
    require_directory(&github)?;
    require_regular(&root.join(CONFIG_PATH), "jactionlint config")?;

    let workflows = Path::new(".github/workflows");
    let workflow_root = root.join(workflows);
    require_directory(&workflow_root)?;
    let mut workflow_count = 0;
    for entry in crate::fs_util::read_dir_sorted(&workflow_root)? {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("reading file type for {}", path.display()))?;
        if file_type.is_symlink() {
            bail!(
                "symlink is not allowed in lint inventory: {}",
                path.display()
            );
        }
        if is_yaml_name(&path) {
            if !file_type.is_file() {
                bail!("non-regular workflow is not allowed: {}", path.display());
            }
            workflow_count += 1;
        } else if legacy_workflow_name(&path) && !file_type.is_file() {
            bail!("non-regular workflow is not allowed: {}", path.display());
        }
    }
    if workflow_count == 0 {
        bail!("no top-level workflow YAML files found in .github/workflows");
    }

    check_dependabot(root)?;
    check_actions_lock(root)?;

    let mut roots = Vec::new();
    collect_action_metadata(root, root, &mut roots)?;
    collect_github_actions(root, Path::new(".github/actions"), true, &mut roots)?;
    select_native_action_metadata(&mut roots);

    let workflows = crate::fs_util::read_dir_sorted(&workflow_root)?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| is_yaml_name(path))
        .collect();
    workflow_lint_uses::check(root, workflows, roots)
}

fn collect_github_actions(
    root: &Path,
    relative: &Path,
    admit_inputs: bool,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    let directory = root.join(relative);
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("reading {}", directory.display()));
        }
    };
    if !metadata.file_type().is_dir() {
        bail!("{} is not a real directory", directory.display());
    }
    for entry in crate::fs_util::read_dir_sorted(&directory)? {
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            bail!(
                "symlink is not allowed in lint inventory: {}",
                path.display()
            );
        }
        if (is_action_metadata(&path) || legacy_action_name(&path)) && !kind.is_file() {
            bail!(
                "non-regular action metadata is not allowed: {}",
                path.display()
            );
        }
        if kind.is_dir() {
            let is_skipped = SKIPPED_DIRS.iter().any(|skip| entry.file_name() == *skip);
            collect_github_actions(
                root,
                &relative.join(entry.file_name()),
                admit_inputs && !is_skipped,
                out,
            )?;
        } else if admit_inputs && is_action_metadata(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn collect_action_metadata(root: &Path, directory: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in crate::fs_util::read_dir_sorted(directory)? {
        let path = entry.path();
        let kind = entry.file_type()?;
        let name = entry.file_name();
        if kind.is_symlink() {
            if is_action_metadata(&path) {
                bail!(
                    "symlink is not allowed in lint inventory: {}",
                    path.display()
                );
            }
            continue;
        }
        if is_action_metadata(&path) && !kind.is_file() {
            bail!(
                "non-regular action metadata is not allowed: {}",
                path.display()
            );
        }
        if kind.is_dir() {
            if SKIPPED_DIRS.iter().any(|skip| name == *skip) {
                continue;
            }
            collect_action_metadata(root, &path, out)?;
        } else if is_action_metadata(&path) {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            let is_workflow_named_action = relative == Path::new(".github/workflows/action.yml")
                || relative == Path::new(".github/workflows/action.yaml");
            if !is_workflow_named_action {
                out.push(path);
            }
        }
    }
    Ok(())
}

fn select_native_action_metadata(paths: &mut Vec<PathBuf>) {
    paths.sort_by(|left, right| {
        left.parent()
            .cmp(&right.parent())
            .then_with(|| action_metadata_rank(left).cmp(&action_metadata_rank(right)))
    });
    let mut selected_parents = BTreeSet::new();
    let mut selected_paths = Vec::new();
    for path in paths.drain(..) {
        let parent = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        if selected_parents.contains(&parent) || fs::read(&path).is_err() {
            continue;
        }
        selected_parents.insert(parent);
        selected_paths.push(path);
    }
    *paths = selected_paths;
}

fn action_metadata_rank(path: &Path) -> u8 {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("action.yml") => 0,
        Some("action.yaml") => 1,
        _ => 2,
    }
}

fn check_dependabot(root: &Path) -> Result<()> {
    let github = root.join(".github");
    for entry in crate::fs_util::read_dir_sorted(&github)? {
        let path = entry.path();
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("dependabot.") {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            bail!(
                "symlink is not allowed in lint inventory: {}",
                path.display()
            );
        }
        if (is_yaml_name(&path) || legacy_dependabot_name(&path)) && !kind.is_file() {
            bail!(
                "non-regular Dependabot config is not allowed: {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn check_actions_lock(root: &Path) -> Result<()> {
    let path = root.join(ACTIONS_LOCK_PATH);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(()),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!(
                "symlink is not allowed in lint inventory: {}",
                path.display()
            )
        }
        Ok(_) => bail!("actions lock is not a regular file: {}", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

fn require_directory(path: &Path) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("reading {}", path.display()))?;
    if !metadata.file_type().is_dir() {
        bail!("{} is not a real directory", path.display());
    }
    Ok(())
}

fn require_regular(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("reading {label} {}", path.display()))?;
    if !metadata.file_type().is_file() {
        bail!("{label} {} is not a regular file", path.display());
    }
    Ok(())
}

fn is_action_metadata(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("action.yml" | "action.yaml")
    )
}

fn legacy_workflow_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("ml") && name.contains(".y"))
}

fn legacy_action_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("action.y") && name.ends_with("ml"))
}

fn legacy_dependabot_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("dependabot.y") && name.ends_with("ml"))
}

fn is_yaml_name(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("yml" | "yaml")
    )
}
