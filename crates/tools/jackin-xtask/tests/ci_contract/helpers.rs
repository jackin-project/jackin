//! File-backed inventory and workflow ownership for the CI contract tests.

use anyhow::{Context, Result, ensure};
use jackin_xtask::workflow_yaml;
use serde_json::Value as YamlValue;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use toml::Value as TomlValue;

#[path = "metadata.rs"]
mod metadata;
use metadata::workspace_package_ids;

#[path = "execution.rs"]
mod execution;

pub(super) fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

pub(super) fn yaml_field<'a>(value: &'a YamlValue, name: &str) -> Option<&'a YamlValue> {
    value.as_object()?.get(name)
}

pub(super) type RustJobOwners = BTreeMap<(String, String), String>;

pub(super) fn workspace_rust_job_owners(root: &Path) -> Result<RustJobOwners> {
    let mut metadata = workspace_package_ids(root)?;
    let configurations = rust_configurations(root)?;
    let excludes = discovery_patterns(root)?;
    let manifest: TomlValue = toml::from_str(&fs::read_to_string(root.join("Cargo.toml"))?)?;
    let members = manifest["workspace"]["members"]
        .as_array()
        .context("workspace members are explicit")?;
    let mut identities = BTreeSet::new();
    let mut members_by_id = BTreeMap::new();
    for member in members {
        let member = member
            .as_str()
            .context("workspace member paths are strings")?;
        ensure!(
            !member.contains('*'),
            "workspace coverage requires explicit member paths"
        );
        let package: TomlValue =
            toml::from_str(&fs::read_to_string(root.join(member).join("Cargo.toml"))?)?;
        let name = package["package"]["name"]
            .as_str()
            .with_context(|| format!("workspace member declares package name: {member}"))?;
        let path = root.join(member).join("Cargo.toml").canonicalize()?;
        let package_id = metadata.remove(&path).with_context(|| {
            format!("file-backed workspace member is in Cargo metadata: {member}")
        })?;
        if excludes
            .iter()
            .any(|pattern| discovery_excludes(member, pattern))
        {
            continue;
        }
        members_by_id.insert(package_id.clone(), member.to_owned());
        for configuration in &configurations {
            ensure!(
                identities.insert((package_id.clone(), name.to_owned(), configuration.clone())),
                "workspace member/configuration is unique: {member}/{configuration}"
            );
        }
    }
    ensure!(!identities.is_empty(), "workspace identities are present");
    ensure!(
        metadata.is_empty(),
        "Cargo workspace has no unaccounted file-backed members"
    );
    velnor_actions_contract::assign_crate_job_ids(&identities, "rust-")
        .into_iter()
        .map(|((package_id, configuration), job)| {
            let member = members_by_id
                .get(&package_id)
                .context("assigned package has a member")?;
            Ok(((member.clone(), configuration), job))
        })
        .collect()
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

fn discovery_patterns(root: &Path) -> Result<Vec<String>> {
    let config: TomlValue = toml::from_str(&fs::read_to_string(root.join(".velnor/config.toml"))?)?;
    config["discovery"]["exclude"]
        .as_array()
        .context("discovery excludes are explicit")?
        .iter()
        .map(|pattern| {
            pattern
                .as_str()
                .map(ToOwned::to_owned)
                .context("discovery exclude patterns are strings")
        })
        .collect()
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

pub(super) fn assert_exact_rust_job_inventory(
    actual: &BTreeSet<String>,
    workspace: &BTreeSet<String>,
) -> Result<()> {
    let expected = workspace;
    let actual = actual
        .iter()
        .filter(|id| id.starts_with("rust-"))
        .cloned()
        .collect::<BTreeSet<_>>();
    ensure!(
        &actual == expected,
        "generated Rust crate/configuration inventory differs: missing {:?}; extra {:?}",
        expected.difference(&actual).collect::<Vec<_>>(),
        actual.difference(expected).collect::<Vec<_>>()
    );
    Ok(())
}

fn rust_configurations(root: &Path) -> Result<Vec<String>> {
    let config: TomlValue = toml::from_str(&fs::read_to_string(root.join(".velnor/config.toml"))?)?;
    let configurations = config["stacks"]["rust"]["configurations"]
        .as_array()
        .context("Rust configurations are explicit")?;
    configurations
        .iter()
        .map(|configuration| {
            configuration["name"]
                .as_str()
                .map(ToOwned::to_owned)
                .context("Rust configuration has a name")
        })
        .collect()
}

pub(super) fn assert_complete_required_fan_in(
    root: &Path,
    jobs: &BTreeSet<String>,
    needs: &BTreeSet<String>,
) -> Result<()> {
    let config: TomlValue = toml::from_str(&fs::read_to_string(root.join(".velnor/config.toml"))?)?;
    let checks = config["checks"]
        .as_array()
        .context("native checks are explicit")?;
    let expected = checks
        .iter()
        .map(|check| {
            check["id"]
                .as_str()
                .map(|id| format!("check-{id}"))
                .context("native check has an ID")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    let actual = jobs
        .iter()
        .filter(|id| id.starts_with("check-"))
        .cloned()
        .collect::<BTreeSet<_>>();
    ensure!(
        expected == actual,
        "every configured native check has exactly one job"
    );
    let expected = jobs
        .iter()
        .filter(|id| !matches!(id.as_str(), "required" | "publish-baseline"))
        .cloned()
        .collect::<BTreeSet<_>>();
    ensure!(
        *needs == expected,
        "Required covers the exact complete job inventory"
    );
    Ok(())
}

pub(super) fn assert_rust_obligation_reports(
    root: &Path,
    workflow: &YamlValue,
    expected_owners: &RustJobOwners,
) -> Result<()> {
    let text = fs::read_to_string(root.join(".github/velnor/task-execution-manifest-v1.json"))?;
    let manifest: YamlValue = serde_json::from_str(
        text.strip_prefix('#')
            .and_then(|text| text.split_once('\n').map(|(_, body)| body))
            .unwrap_or(&text),
    )?;
    let tasks = manifest["tasks"]
        .as_object()
        .context("task manifest has tasks")?;
    let jobs = workflow["jobs"].as_object().context("workflow has jobs")?;
    execution::assert_rust_execution_inventory(tasks, jobs, expected_owners)?;
    for (id, job) in jobs.iter().filter(|(id, _)| id.starts_with("rust-")) {
        let steps = job["steps"].as_array().context("crate job has steps")?;
        for name in [
            "Build test executables",
            "Format",
            "Clippy",
            "Unit and integration tests",
            "Documentation",
            "Wait for parallel checks",
            "Upload crate reports",
        ] {
            ensure!(
                steps
                    .iter()
                    .filter(|step| step["name"].as_str() == Some(name))
                    .count()
                    == 1,
                "crate job retains exactly one {name}: {id}"
            );
        }
        ensure!(
            steps
                .iter()
                .any(|step| step["name"] == "Upload crate reports" && step["if"] == "always()"),
            "crate report upload runs after failure too: {id}"
        );
    }
    Ok(())
}

fn expected_task_owner<'a>(id: &str, owners: &'a RustJobOwners) -> Result<&'a String> {
    let (member, configuration) = id
        .strip_prefix("stack/rust/")
        .context("Rust task prefix")?
        .rsplit_once('/')
        .context("Rust task configuration")?;
    let (member, _) = member.rsplit_once('/').context("Rust task kind")?;
    owners
        .get(&(member.to_owned(), configuration.to_owned()))
        .with_context(|| format!("Rust task names a declared member/configuration: {id}"))
}
