use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use super::is_yaml_name;

pub(super) fn check(
    root: &Path,
    workflows: Vec<PathBuf>,
    action_roots: Vec<PathBuf>,
) -> Result<()> {
    let mut pending = workflows
        .into_iter()
        .map(|path| (path, InputKind::Workflow))
        .collect::<VecDeque<_>>();
    pending.extend(
        action_roots
            .into_iter()
            .map(|path| (path, InputKind::Action)),
    );
    check_local_action_closure(root, pending)
}

#[derive(Clone, Copy)]
enum InputKind {
    Workflow,
    Action,
}

fn check_local_action_closure(
    root: &Path,
    mut pending: VecDeque<(PathBuf, InputKind)>,
) -> Result<()> {
    let mut visited = BTreeSet::new();
    while let Some((path, kind)) = pending.pop_front() {
        let relative = path
            .strip_prefix(root)
            .with_context(|| format!("lint input {} is outside the repository", path.display()))?;
        if !visited.insert(relative.to_path_buf()) {
            continue;
        }
        let bytes =
            fs::read(&path).with_context(|| format!("reading lint input {}", path.display()))?;
        let document = crate::workflow_yaml::parse(&bytes)
            .with_context(|| format!("parsing local uses in {}", path.display()))?;
        for target in local_uses(&document, kind) {
            if let Some(input) = resolve_local_input(root, &target)? {
                pending.push_back(input);
            }
        }
    }
    Ok(())
}

fn local_uses(document: &serde_json::Value, kind: InputKind) -> Vec<String> {
    let mut uses = Vec::new();
    match kind {
        InputKind::Workflow => {
            if let Some(jobs) = document.get("jobs").and_then(serde_json::Value::as_object) {
                for job in jobs.values() {
                    if let Some(target) = job.get("uses").and_then(serde_json::Value::as_str) {
                        uses.push(target.to_owned());
                    }
                    collect_step_uses(job.get("steps"), &mut uses);
                }
            }
        }
        InputKind::Action => {
            let runs = document.get("runs");
            if runs
                .and_then(|value| value.get("using"))
                .and_then(serde_json::Value::as_str)
                == Some("composite")
            {
                collect_step_uses(runs.and_then(|value| value.get("steps")), &mut uses);
            }
        }
    }
    uses
}

fn collect_step_uses(steps: Option<&serde_json::Value>, uses: &mut Vec<String>) {
    if let Some(steps) = steps.and_then(serde_json::Value::as_array) {
        uses.extend(steps.iter().filter_map(|step| {
            step.get("uses")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        }));
    }
}

fn resolve_local_input(root: &Path, target: &str) -> Result<Option<(PathBuf, InputKind)>> {
    let Some(relative) = target
        .strip_prefix("./")
        .or_else(|| target.strip_prefix("$/"))
    else {
        return Ok(None);
    };
    let relative = safe_relative_path(root, relative)?;
    let path = root.join(&relative);
    let Some(metadata) = checked_metadata(root, &relative)? else {
        return Ok(None);
    };
    if is_yaml_name(&path) {
        if !metadata.file_type().is_file() {
            bail!(
                "local workflow input is not a regular file: {}",
                path.display()
            );
        }
        return Ok(Some((path, InputKind::Workflow)));
    }
    if metadata.file_type().is_file() {
        return Ok(Some((path, InputKind::Action)));
    }
    if !metadata.file_type().is_dir() {
        bail!(
            "local action input is not a regular file or directory: {}",
            path.display()
        );
    }
    for name in ["action.yml", "action.yaml"] {
        let candidate = relative.join(name);
        if let Some(metadata) = checked_metadata(root, &candidate)? {
            if !metadata.file_type().is_file() {
                bail!(
                    "local action metadata is not a regular file: {}",
                    root.join(&candidate).display()
                );
            }
            return Ok(Some((root.join(candidate), InputKind::Action)));
        }
    }
    Ok(None)
}

fn safe_relative_path(root: &Path, value: &str) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    let components = Path::new(value).components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        match component {
            Component::Normal(part) => {
                normalized.push(part);
                match fs::symlink_metadata(root.join(&normalized)) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        bail!(
                            "symlink is not allowed in lint inventory: {}",
                            root.join(&normalized).display()
                        );
                    }
                    Ok(metadata)
                        if index + 1 < components.len() && !metadata.file_type().is_dir() =>
                    {
                        bail!(
                            "local action path component is not a directory: {}",
                            root.join(&normalized).display()
                        );
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!("reading {}", root.join(&normalized).display())
                        });
                    }
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    bail!("local action path escapes the repository: {value}");
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                bail!("local action path is not repository-relative: {value}");
            }
        }
    }
    Ok(normalized)
}

fn checked_metadata(root: &Path, relative: &Path) -> Result<Option<fs::Metadata>> {
    if relative.as_os_str().is_empty() {
        let metadata = fs::symlink_metadata(root)
            .with_context(|| format!("reading repository root {}", root.display()))?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            bail!(
                "repository root is not a real directory: {}",
                root.display()
            );
        }
        return Ok(Some(metadata));
    }
    let mut current = PathBuf::new();
    let components = relative.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            bail!(
                "invalid local action path component in {}",
                relative.display()
            );
        };
        current.push(name);
        let path = root.join(&current);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
        };
        if metadata.file_type().is_symlink() {
            bail!(
                "symlink is not allowed in lint inventory: {}",
                path.display()
            );
        }
        if index + 1 < components.len() && !metadata.file_type().is_dir() {
            bail!(
                "local action path component is not a directory: {}",
                path.display()
            );
        }
        if index + 1 == components.len() {
            return Ok(Some(metadata));
        }
    }
    Ok(None)
}
