//! File-backed inventory and workflow ownership for the CI contract tests.

use anyhow::{Context, Result, ensure};
use jackin_xtask::workflow_yaml;
use serde_json::Value as YamlValue;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use toml::Value as TomlValue;

pub(super) fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

pub(super) fn yaml_field<'a>(value: &'a YamlValue, name: &str) -> Option<&'a YamlValue> {
    value.as_object()?.get(name)
}

pub(super) fn workspace_rust_job_ids(root: &Path) -> Result<BTreeSet<String>> {
    let manifest: TomlValue = toml::from_str(
        &fs::read_to_string(root.join("Cargo.toml")).context("workspace manifest exists")?,
    )
    .context("workspace manifest is valid TOML")?;
    let members = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(TomlValue::as_array)
        .context("workspace members are explicit")?;
    let mut jobs = BTreeSet::new();
    for member in members {
        let member = member
            .as_str()
            .context("workspace member paths are strings")?;
        ensure!(
            !member.contains('*'),
            "workspace coverage contract requires explicit member paths"
        );
        let package_manifest = root.join(member).join("Cargo.toml");
        let package: TomlValue = toml::from_str(
            &fs::read_to_string(&package_manifest)
                .with_context(|| format!("workspace package manifest exists: {member}"))?,
        )
        .with_context(|| format!("workspace package manifest is valid TOML: {member}"))?;
        let name = package
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(TomlValue::as_str)
            .with_context(|| format!("workspace member declares package name: {member}"))?;
        for phase in ["lint", "tests", "docs"] {
            ensure!(
                jobs.insert(format!("rust-{name}-{phase}")),
                "workspace phase job ID is unique: {name}/{phase}"
            );
        }
    }
    ensure!(!jobs.is_empty(), "workspace members are present");
    Ok(jobs)
}

pub(super) fn workflow_jobs(root: &Path) -> Result<(YamlValue, BTreeSet<String>)> {
    let workflow_bytes =
        fs::read(root.join(".github/workflows/ci.yml")).context("generated CI workflow exists")?;
    let workflow: YamlValue =
        workflow_yaml::parse(&workflow_bytes).context("generated CI workflow is valid YAML")?;
    let jobs = yaml_field(&workflow, "jobs")
        .and_then(YamlValue::as_object)
        .context("generated CI workflow declares jobs")?;
    let ids = jobs.keys().cloned().collect::<BTreeSet<_>>();
    Ok((workflow, ids))
}

fn discovery_excludes(member: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/**") {
        if let Some(name) = prefix.strip_prefix("**/") {
            return member.split('/').any(|component| component == name);
        }
        return member == prefix || member.starts_with(&format!("{prefix}/"));
    }
    member == pattern || member.starts_with(&format!("{pattern}/"))
}

pub(super) fn discovery_excluded_job_ids(root: &Path) -> Result<BTreeSet<String>> {
    let config: TomlValue = toml::from_str(
        &fs::read_to_string(root.join(".velnor/config.toml")).context("Velnor config exists")?,
    )
    .context("Velnor config is valid TOML")?;
    let patterns = config
        .get("discovery")
        .and_then(|discovery| discovery.get("exclude"))
        .and_then(TomlValue::as_array)
        .context("discovery excludes are explicit")?
        .iter()
        .map(|pattern| {
            pattern
                .as_str()
                .context("discovery exclude patterns are strings")
        })
        .collect::<Result<Vec<_>>>()?;
    let manifest: TomlValue = toml::from_str(
        &fs::read_to_string(root.join("Cargo.toml")).context("workspace manifest exists")?,
    )
    .context("workspace manifest is valid TOML")?;
    let members = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(TomlValue::as_array)
        .context("workspace members are explicit")?;
    let mut jobs = BTreeSet::new();
    for member in members {
        let member = member
            .as_str()
            .context("workspace member paths are strings")?;
        if !patterns
            .iter()
            .any(|pattern| discovery_excludes(member, pattern))
        {
            continue;
        }
        let package_manifest = root.join(member).join("Cargo.toml");
        let package: TomlValue = toml::from_str(&fs::read_to_string(&package_manifest)?)
            .with_context(|| format!("workspace package manifest is valid TOML: {member}"))?;
        let name = package
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(TomlValue::as_str)
            .with_context(|| format!("workspace member declares package name: {member}"))?;
        for phase in ["lint", "tests", "docs"] {
            jobs.insert(format!("rust-{name}-{phase}"));
        }
    }
    Ok(jobs)
}

pub(super) fn configured_workflow_tasks(root: &Path) -> Result<Vec<TomlValue>> {
    let config: TomlValue = toml::from_str(
        &fs::read_to_string(root.join("docs/integration/unpublished-workflow-tasks.toml"))
            .context("unpublished workflow tasks exist")?,
    )
    .context("unpublished workflow tasks are valid TOML")?;
    let tasks = config
        .get("workflow")
        .and_then(|workflow| workflow.get("tasks"))
        .and_then(TomlValue::as_array)
        .context("Velnor declares workflow task coverage")?;
    let mut previous = None;
    let mut ids = BTreeSet::new();
    for task in tasks {
        let id = task
            .get("id")
            .and_then(TomlValue::as_str)
            .context("workflow task has an ID")?;
        ensure!(ids.insert(id), "workflow task IDs are unique");
        ensure!(
            previous.is_none_or(|previous: &str| previous < id),
            "workflow tasks are sorted by ID"
        );
        previous = Some(id);
    }
    Ok(tasks.clone())
}

pub(super) fn configured_task_source(task: &TomlValue) -> Result<(&str, &str)> {
    let source = task
        .get("source")
        .and_then(TomlValue::as_table)
        .context("workflow task has an explicit source")?;
    let mise_config = source
        .get("mise_config")
        .and_then(TomlValue::as_str)
        .context("workflow task source has a Mise config path")?;
    let working_directory = source
        .get("working_directory")
        .and_then(TomlValue::as_str)
        .context("workflow task source has a working directory")?;
    Ok((mise_config, working_directory))
}

pub(super) fn job_needs(job: &YamlValue) -> Result<BTreeSet<String>> {
    yaml_field(job, "needs")
        .and_then(YamlValue::as_array)
        .into_iter()
        .flatten()
        .map(|need| {
            need.as_str()
                .map(ToOwned::to_owned)
                .context("job needs entries are strings")
        })
        .collect::<Result<BTreeSet<_>>>()
}

pub(super) fn task_job_ids(workflow_job_ids: &BTreeSet<String>, task_id: &str) -> Vec<String> {
    let prefix = format!("task-{task_id}");
    workflow_job_ids
        .iter()
        .filter(|job_id| {
            *job_id == &prefix
                || job_id
                    .strip_prefix(&prefix)
                    .is_some_and(|suffix| matches!(suffix, "-hosted" | "-scale-set"))
        })
        .cloned()
        .collect()
}

pub(super) fn core_string_constant<'a>(contents: &'a str, name: &str) -> Result<&'a str> {
    let marker = format!("pub const {name}: &str = \"");
    contents
        .split_once(marker.as_str())
        .and_then(|(_, rest)| rest.split_once('"').map(|(value, _)| value))
        .with_context(|| format!("Jackin declares {name}"))
}

pub(super) fn assert_exact_rust_phase_inventory(
    actual: &BTreeSet<String>,
    workspace: &BTreeSet<String>,
    excluded: &BTreeSet<String>,
) -> Result<()> {
    let expected = workspace
        .difference(excluded)
        .cloned()
        .collect::<BTreeSet<_>>();
    let actual = actual
        .iter()
        .filter(|id| id.starts_with("rust-"))
        .cloned()
        .collect::<BTreeSet<_>>();
    ensure!(
        actual == expected,
        "generated Rust phase inventory differs: missing {:?}; extra {:?}",
        expected.difference(&actual).collect::<Vec<_>>(),
        actual.difference(&expected).collect::<Vec<_>>()
    );
    Ok(())
}
