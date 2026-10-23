//! Authority checks for the optional dependency-seed job and its local actions.

use anyhow::{Context, Result, ensure};
use jackin_xtask::workflow_yaml;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

const MAX_COMPOSITE_DEPTH: usize = 32;
const MAX_COMPOSITE_ACTIONS: usize = 128;
const MAX_ACTION_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_TOTAL_METADATA_BYTES: u64 = 8 * 1024 * 1024;

#[cfg(test)]
#[path = "seed_authority/tests.rs"]
mod tests;

pub(super) fn assert_steps(root: &Path, steps: &[Value]) -> Result<()> {
    let mut completed = BTreeSet::new();
    let mut active = BTreeSet::new();
    let mut metadata_bytes = 0;
    assert_step_list(
        root,
        steps,
        0,
        &mut active,
        &mut completed,
        &mut metadata_bytes,
    )
}

fn assert_step_list(
    root: &Path,
    steps: &[Value],
    depth: usize,
    active: &mut BTreeSet<PathBuf>,
    completed: &mut BTreeSet<PathBuf>,
    metadata_bytes: &mut u64,
) -> Result<()> {
    for step in steps {
        ensure!(
            step.get("env")
                .and_then(Value::as_object)
                .is_none_or(|env| {
                    !env.contains_key("VELNOR_INTERNAL_OP")
                        && !env.contains_key("VELNOR_TASK_EXECUTION_DIGEST")
                })
                && step
                    .get("with")
                    .and_then(Value::as_object)
                    .is_none_or(|with| !with.contains_key("digest"))
                && !step.get("run").and_then(Value::as_str).is_some_and(|run| {
                    run.contains("VELNOR_INTERNAL_OP")
                        || run.contains("VELNOR_TASK_EXECUTION_DIGEST")
                })
                && !step
                    .get("uses")
                    .and_then(Value::as_str)
                    .is_some_and(|uses| {
                        uses.contains("declared-task-") || uses.contains("upload-artifact")
                    }),
            "optional dependency seed cannot own task execution or report artifacts"
        );

        if let Some(uses) = step.get("uses").and_then(Value::as_str)
            && let Some(action) = local_action_path(uses)?
        {
            assert_composite(root, &action, depth + 1, active, completed, metadata_bytes)?;
        }
    }
    Ok(())
}

fn local_action_path(uses: &str) -> Result<Option<PathBuf>> {
    ensure!(
        !uses.starts_with("$/"),
        "optional dependency seed cannot use repository-root action aliases"
    );
    if !uses.starts_with("./") {
        return Ok(None);
    }
    let Some(name) = uses.strip_prefix("./.github/actions/") else {
        anyhow::bail!("optional dependency seed local action path is outside the action namespace");
    };
    ensure!(
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')),
        "optional dependency seed local action path is one safe action name"
    );
    Ok(Some(PathBuf::from(".github/actions").join(name)))
}

fn assert_composite(
    root: &Path,
    relative: &Path,
    depth: usize,
    active: &mut BTreeSet<PathBuf>,
    completed: &mut BTreeSet<PathBuf>,
    metadata_bytes: &mut u64,
) -> Result<()> {
    ensure!(
        relative
            .components()
            .all(|component| matches!(component, Component::Normal(_))),
        "local composite action path has no traversal components"
    );
    if completed.contains(relative) {
        return Ok(());
    }
    ensure!(
        depth <= MAX_COMPOSITE_DEPTH,
        "local composite action nesting is within the depth budget"
    );
    ensure!(
        active.len() + completed.len() < MAX_COMPOSITE_ACTIONS,
        "local composite action closure is within the action-count budget"
    );
    ensure!(
        active.insert(relative.to_owned()),
        "local composite action references cannot form a cycle"
    );

    let result = (|| {
        let directory = root.join(relative);
        assert_no_symlink_components(root, relative)?;
        let metadata = fs::symlink_metadata(&directory)
            .with_context(|| format!("local composite action exists: {}", directory.display()))?;
        ensure!(
            metadata.file_type().is_dir(),
            "local composite action target is a directory"
        );
        let definition = [directory.join("action.yml"), directory.join("action.yaml")];
        let mut definitions = Vec::new();
        for path in &definition {
            match fs::symlink_metadata(path) {
                Ok(metadata) => {
                    ensure!(
                        metadata.file_type().is_file(),
                        "local composite action metadata is a regular file"
                    );
                    definitions.push(path);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error).context("local composite action metadata exists"),
            }
        }
        ensure!(
            definitions.len() == 1,
            "local composite action has exactly one action.yml or action.yaml"
        );
        let action_path = definitions[0];
        let action_metadata = fs::symlink_metadata(action_path)
            .context("local composite action metadata is readable")?;
        ensure!(
            action_metadata.file_type().is_file(),
            "local composite action metadata is a regular file"
        );
        let remaining_total = MAX_TOTAL_METADATA_BYTES - *metadata_bytes;
        let file_limit = MAX_ACTION_METADATA_BYTES.min(remaining_total);
        let bytes = read_action_metadata(action_path, file_limit)?;
        ensure!(
            bytes.len() as u64 <= file_limit,
            "local composite action metadata is within the remaining byte budget"
        );
        *metadata_bytes = metadata_bytes
            .checked_add(bytes.len() as u64)
            .context("local composite action metadata byte count does not overflow")?;
        ensure!(
            *metadata_bytes <= MAX_TOTAL_METADATA_BYTES,
            "local composite action closure is within the aggregate byte budget"
        );
        let action = workflow_yaml::parse(&bytes)
            .context("local composite action metadata is valid bounded YAML")?;
        ensure!(
            action["runs"]["using"] == "composite",
            "local seed action uses composite metadata"
        );
        let steps = action["runs"]["steps"]
            .as_array()
            .context("local composite action has a steps array")?;
        assert_step_list(root, steps, depth, active, completed, metadata_bytes)
    })();

    active.remove(relative);
    result?;
    completed.insert(relative.to_owned());
    Ok(())
}

#[expect(
    clippy::disallowed_methods,
    reason = "bounded local composite metadata reads run synchronously on the CI integration-test thread, never a runtime or rendering thread"
)]
fn read_action_metadata(path: &Path, max_bytes: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .context("local composite action metadata is readable")?
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .context("local composite action metadata is readable")?;
    Ok(bytes)
}

fn assert_no_symlink_components(root: &Path, relative: &Path) -> Result<()> {
    let mut current = root.to_owned();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            anyhow::bail!("local composite action path has no traversal components");
        };
        current.push(component);
        let metadata = fs::symlink_metadata(&current).with_context(|| {
            format!("local composite action path exists: {}", current.display())
        })?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "local composite action path has no symlink component"
        );
    }
    Ok(())
}
