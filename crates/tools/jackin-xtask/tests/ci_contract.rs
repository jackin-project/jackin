use anyhow::{Context, Result, ensure};
use serde_json::Value as YamlValue;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs};
use toml::Value as TomlValue;

const ARCHITECT_REPOSITORY: &str = "jackin-project/jackin-the-architect";
const ARCHITECT_COMMIT: &str = "7db69b62f598a0971809ee4a006ad3f5477d0996";
const ARCHITECT_MANIFEST_SHA256: &str =
    "b38e506587c98137d0a1a88247fb68afc9f9f215c8104c838df251933a917ae0";

#[path = "ci_contract/helpers.rs"]
mod helpers;
use helpers::{
    assert_complete_required_fan_in, assert_exact_rust_job_inventory,
    assert_rust_obligation_reports, configured_task_source, configured_workflow_tasks,
    core_string_constant, job_needs, repository_root, task_job_ids, workflow_jobs,
    workspace_rust_job_owners, yaml_field,
};

#[test]
fn required_fan_in_covers_every_workspace_crate_and_configured_task() -> Result<()> {
    let root = repository_root();
    let owners = workspace_rust_job_owners(&root)?;
    let workspace_jobs = owners.values().cloned().collect();
    let active =
        fs::read_to_string(root.join(".velnor/config.toml")).context("Velnor config exists")?;
    ensure!(
        !active.contains("[[workflow.tasks]]"),
        "active Velnor config does not publish workflow tasks the generator rejects"
    );
    let preserved =
        fs::read_to_string(root.join("docs/integration/unpublished-workflow-tasks.toml"))
            .context("unpublished workflow tasks exist")?;
    for id in [
        "construct-upstream-assets",
        "native-desktop-ci",
        "native-swift-format",
        "native-swiftlint",
    ] {
        ensure!(
            preserved.contains(&format!("id = \"{id}\"")),
            "unpublished workflow task remains recorded: {id}"
        );
    }
    let (workflow, job_ids) = workflow_jobs(&root)?;
    let jobs = yaml_field(&workflow, "jobs")
        .and_then(YamlValue::as_object)
        .context("generated CI workflow declares jobs")?;
    ensure!(
        !job_ids.iter().any(|id| id.starts_with("task-")),
        "unpublished workflow tasks are not emitted as partial jobs"
    );

    assert_exact_rust_job_inventory(&job_ids, &workspace_jobs)?;
    assert_rust_obligation_reports(&root, &workflow, &owners)?;
    for job_id in &workspace_jobs {
        ensure!(
            job_ids.contains(job_id),
            "workspace member/configuration has its generated Rust job: {job_id}"
        );
    }

    let required = jobs
        .get("required")
        .context("generated workflow has the Required gate")?;
    let required_needs = job_needs(required)?;
    assert_complete_required_fan_in(&root, &job_ids, &required_needs)?;
    for job_id in &workspace_jobs {
        ensure!(
            required_needs.contains(job_id),
            "Required waits for covered job {job_id}"
        );
    }
    Ok(())
}

#[test]
fn configured_verification_jobs_are_isolated_and_run_only_the_declared_mise_task() -> Result<()> {
    let root = repository_root();
    let tasks = configured_workflow_tasks(&root)?;
    let (_workflow, job_ids) = workflow_jobs(&root)?;
    let expected_native = BTreeSet::from([
        "construct-upstream-assets",
        "native-swift-format",
        "native-swiftlint",
    ]);
    let configured_native = tasks
        .iter()
        .filter(|task| task.get("kind").and_then(TomlValue::as_str) == Some("verification"))
        .map(|task| {
            task.get("id")
                .and_then(TomlValue::as_str)
                .context("verification task has an ID")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        configured_native == expected_native,
        "the upstream asset, Swift format, and SwiftLint verification tasks remain configured"
    );

    for task in tasks
        .iter()
        .filter(|task| task.get("kind").and_then(TomlValue::as_str) == Some("verification"))
    {
        let id = task
            .get("id")
            .and_then(TomlValue::as_str)
            .context("verification task has an ID")?;
        let expected_source = match id {
            "construct-upstream-assets" => ("mise.toml", "."),
            "native-swift-format" | "native-swiftlint" => ("native/mise.toml", "native"),
            other => anyhow::bail!("unexpected configured verification task: {other}"),
        };
        ensure!(
            configured_task_source(task)? == expected_source,
            "verification task keeps its declared source config and working directory: {id}"
        );
        let mise_task = task
            .get("mise_task")
            .and_then(TomlValue::as_str)
            .context("verification task declares an exact Mise task")?;
        let runner = task
            .get("runner")
            .and_then(TomlValue::as_str)
            .context("verification task declares its runner")?;
        let expected_runner = match runner {
            "linux-x64" => "ubuntu-26.04",
            "macos-arm64" => "macos-15",
            "macos-26-arm64" => "macos-26",
            other => anyhow::bail!("unsupported configured verification runner: {other}"),
        };
        let expected_mise_task = match id {
            "construct-upstream-assets" => "renovate-upstream-sources",
            "native-swift-format" => "format-check",
            "native-swiftlint" => "lint",
            other => anyhow::bail!("unexpected configured verification task: {other}"),
        };
        ensure!(
            mise_task == expected_mise_task,
            "verification task keeps its declared Mise task: {id}"
        );
        ensure!(
            !expected_runner.is_empty(),
            "verification runner {runner} maps to a hosted label for {id}"
        );
        ensure!(
            task_job_ids(&job_ids, id).is_empty(),
            "unpublished verification task is not emitted: {id}"
        );
    }
    Ok(())
}

#[test]
fn configured_native_build_is_mac26_bounded_locked_and_mbx_routed() -> Result<()> {
    let root = repository_root();
    let tasks = configured_workflow_tasks(&root)?;
    let task = tasks
        .iter()
        .find(|task| task.get("id").and_then(TomlValue::as_str) == Some("native-desktop-ci"))
        .context("native desktop build task is configured")?;
    ensure!(
        task.get("kind").and_then(TomlValue::as_str) == Some("build"),
        "native desktop cadence remains a build task"
    );
    ensure!(
        task.get("mise_task").and_then(TomlValue::as_str) == Some("ci"),
        "the generated job executes the native ci task"
    );
    ensure!(
        task.get("runner").and_then(TomlValue::as_str) == Some("macos-26-arm64"),
        "native build runs on the pinned macOS 26 ARM64 runner"
    );
    ensure!(
        configured_task_source(task)? == ("native/mise.toml", "native"),
        "native build preserves its declared Mise config and working directory"
    );
    ensure!(
        task.get("cargo_build_jobs").and_then(TomlValue::as_integer) == Some(2)
            && task
                .get("nextest_test_threads")
                .and_then(TomlValue::as_integer)
                == Some(2),
        "native build and test parallelism remain capped at two"
    );
    let expected_tools = [
        "aqua:nextest-rs/nextest/cargo-nextest",
        "github:boltffi/boltffi",
        "mr-boxington",
        "rust",
        "swiftlint",
        "xcodegen",
    ];
    let tools = task
        .get("tools")
        .and_then(TomlValue::as_array)
        .context("native build tool closure is explicit")?
        .iter()
        .map(|tool| tool.as_str().context("tool keys are strings"))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        tools == expected_tools,
        "native build tools are pinned and sorted"
    );

    let root_mise: TomlValue = toml::from_str(
        &fs::read_to_string(root.join("mise.toml")).context("root Mise config exists")?,
    )
    .context("root Mise config is valid TOML")?;
    let root_tools = root_mise
        .get("tools")
        .and_then(TomlValue::as_table)
        .context("root tool selectors are explicit")?;
    ensure!(
        !root_tools.contains_key("cargo:boltffi_cli"),
        "the retired cargo-binstall Boltffi route is absent"
    );
    let boltffi = root_tools
        .get("github:boltffi/boltffi")
        .context("Boltffi uses its pinned upstream release")?;
    ensure!(
        boltffi.get("version").and_then(TomlValue::as_str) == Some("0.31.1"),
        "Boltffi version matches its desktop dependency"
    );
    let native_mise: TomlValue = toml::from_str(
        &fs::read_to_string(root.join("native/mise.toml"))
            .context("native Mise task source exists")?,
    )
    .context("native Mise task source is valid TOML")?;
    let native_ci = native_mise
        .get("tasks")
        .and_then(|tasks| tasks.get("ci"))
        .and_then(TomlValue::as_table)
        .context("native ci task is declared")?;
    ensure!(
        native_ci.get("dir").is_none(),
        "declared native working directory is not overridden by a task alias"
    );
    let ci_script = native_ci
        .get("run")
        .and_then(TomlValue::as_str)
        .context("native ci task has an inline script")?;
    ensure!(
        ci_script.contains("mbx +1.99.1 xtask desktop")
            && !ci_script
                .lines()
                .any(|line| line.trim_start().starts_with("cargo ")),
        "native Rust commands route through the pinned MBX entry point"
    );

    let (_workflow, job_ids) = workflow_jobs(&root)?;
    ensure!(
        task_job_ids(&job_ids, "native-desktop-ci").is_empty(),
        "native desktop CI stays unpublished until source-qualified tasks can be generated"
    );
    ensure!(
        task.get("timeout_minutes").and_then(TomlValue::as_integer) == Some(120),
        "native build retains its bounded timeout"
    );
    Ok(())
}

#[test]
fn architect_manifest_snapshot_is_bound_to_an_immutable_source_commit() -> Result<()> {
    let root = repository_root();
    let fixture = root.join("crates/tools/jackin-xtask/tests/fixtures/architect");
    let provenance: TomlValue = toml::from_str(
        &fs::read_to_string(fixture.join("provenance.toml")).context("role provenance exists")?,
    )
    .context("role provenance is valid TOML")?;
    let repository = provenance
        .get("repository")
        .and_then(TomlValue::as_str)
        .context("role repository is pinned")?;
    let commit = provenance
        .get("commit")
        .and_then(TomlValue::as_str)
        .context("role commit is pinned")?;
    let source_path = provenance
        .get("path")
        .and_then(TomlValue::as_str)
        .context("role manifest path is pinned")?;
    let expected_sha256 = provenance
        .get("sha256")
        .and_then(TomlValue::as_str)
        .context("role manifest digest is pinned")?;
    let source_url = provenance
        .get("url")
        .and_then(TomlValue::as_str)
        .context("immutable source URL is pinned")?;

    ensure!(
        repository == ARCHITECT_REPOSITORY,
        "Architect repository pin changed"
    );
    ensure!(commit == ARCHITECT_COMMIT, "Architect commit pin changed");
    let core_constants = fs::read_to_string(root.join("crates/core/jackin-core/src/constants.rs"))
        .context("Jackin manifest constants exist")?;
    let current_manifest_filename = core_string_constant(&core_constants, "MANIFEST_FILENAME")?;
    ensure!(
        source_path == current_manifest_filename,
        "Architect manifest path differs from Jackin's current MANIFEST_FILENAME"
    );
    ensure!(
        expected_sha256 == ARCHITECT_MANIFEST_SHA256,
        "Architect manifest digest pin changed"
    );
    ensure!(
        source_url == format!("https://github.com/{repository}/blob/{commit}/{source_path}"),
        "Architect source URL is not the immutable blob URL"
    );
    ensure!(
        commit.len() == 40,
        "source revision must be a full Git commit ID"
    );
    ensure!(
        commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "source revision must be hexadecimal"
    );

    let manifest = fs::read(fixture.join(current_manifest_filename))
        .context("pinned role manifest exists at Jackin's current manifest path")?;
    let actual_sha256 = hex::encode(Sha256::digest(&manifest));
    ensure!(
        actual_sha256 == expected_sha256,
        "pinned role content changed"
    );

    let manifest_text = std::str::from_utf8(&manifest).context("pinned role manifest is UTF-8")?;
    let manifest: TomlValue =
        toml::from_str(manifest_text).context("pinned role manifest is valid TOML")?;
    let manifest_version = manifest
        .get("version")
        .and_then(TomlValue::as_str)
        .context("role manifest declares its version")?;
    let current_version = core_string_constant(&core_constants, "CURRENT_MANIFEST_VERSION")?;
    ensure!(
        manifest_version == current_version,
        "pinned Architect manifest version differs from Jackin"
    );

    let agents = manifest
        .get("agents")
        .and_then(TomlValue::as_array)
        .context("Architect declares its supported agents")?;
    let actual_agents = agents
        .iter()
        .map(TomlValue::as_str)
        .collect::<Option<Vec<_>>>()
        .context("agent names are strings")?;
    ensure!(
        actual_agents == ["claude", "codex", "amp", "opencode", "kimi", "grok"],
        "Architect supported-agent manifest changed: {actual_agents:?}"
    );
    Ok(())
}
