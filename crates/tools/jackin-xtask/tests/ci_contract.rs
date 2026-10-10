use anyhow::{Context, Result, ensure};
use serde_yaml_ng::Value as YamlValue;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use toml::Value as TomlValue;

const ARCHITECT_REPOSITORY: &str = "jackin-project/jackin-the-architect";
const ARCHITECT_COMMIT: &str = "7db69b62f598a0971809ee4a006ad3f5477d0996";
const ARCHITECT_MANIFEST_SHA256: &str =
    "b38e506587c98137d0a1a88247fb68afc9f9f215c8104c838df251933a917ae0";

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn yaml_field<'a>(value: &'a YamlValue, name: &str) -> Option<&'a YamlValue> {
    value.as_mapping()?.get(&YamlValue::String(name.to_owned()))
}

fn workspace_rust_job_ids(root: &Path) -> Result<BTreeSet<String>> {
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
        ensure!(
            jobs.insert(format!("rust-{name}")),
            "workspace package job ID is unique: {name}"
        );
    }
    ensure!(!jobs.is_empty(), "workspace members are present");
    Ok(jobs)
}

fn workflow_jobs(root: &Path) -> Result<(YamlValue, BTreeSet<String>)> {
    let workflow: YamlValue = serde_yaml_ng::from_slice(
        &fs::read(root.join(".github/workflows/ci.yml")).context("generated CI workflow exists")?,
    )
    .context("generated CI workflow is valid YAML")?;
    let jobs = yaml_field(&workflow, "jobs")
        .and_then(YamlValue::as_mapping)
        .context("generated CI workflow declares jobs")?;
    let ids = jobs
        .keys()
        .map(|key| {
            key.as_str()
                .map(ToOwned::to_owned)
                .context("workflow job IDs are strings")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    Ok((workflow, ids))
}

fn configured_workflow_tasks(root: &Path) -> Result<Vec<TomlValue>> {
    let config: TomlValue = toml::from_str(
        &fs::read_to_string(root.join(".velnor/config.toml")).context("Velnor config exists")?,
    )
    .context("Velnor config is valid TOML")?;
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

fn configured_task_source(task: &TomlValue) -> Result<(&str, &str)> {
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

fn job_needs(job: &YamlValue) -> Result<BTreeSet<String>> {
    Ok(yaml_field(job, "needs")
        .and_then(YamlValue::as_sequence)
        .into_iter()
        .flatten()
        .map(|need| {
            need.as_str()
                .map(ToOwned::to_owned)
                .context("job needs entries are strings")
        })
        .collect::<Result<BTreeSet<_>>>()?)
}

fn task_job_ids(workflow_job_ids: &BTreeSet<String>, task_id: &str) -> Vec<String> {
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

fn yaml_strings(value: &YamlValue) -> Vec<&str> {
    match value {
        YamlValue::String(value) => vec![value],
        YamlValue::Sequence(values) => values.iter().flat_map(yaml_strings).collect(),
        YamlValue::Mapping(values) => values.values().flat_map(yaml_strings).collect(),
        _ => Vec::new(),
    }
}

fn job_run_scripts<'a>(job: &'a YamlValue) -> Vec<&'a str> {
    yaml_field(job, "steps")
        .and_then(YamlValue::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(|step| yaml_field(step, "run"))
        .flat_map(yaml_strings)
        .collect()
}

fn core_string_constant<'a>(contents: &'a str, name: &str) -> Result<&'a str> {
    let marker = format!("pub const {name}: &str = \"");
    contents
        .split_once(marker.as_str())
        .and_then(|(_, rest)| rest.split_once('"').map(|(value, _)| value))
        .with_context(|| format!("Jackin declares {name}"))
}

#[test]
fn required_fan_in_covers_every_workspace_crate_and_configured_task() -> Result<()> {
    let root = repository_root();
    let workspace_jobs = workspace_rust_job_ids(&root)?;
    let task_configs = configured_workflow_tasks(&root)?;
    let (workflow, job_ids) = workflow_jobs(&root)?;
    let jobs = yaml_field(&workflow, "jobs")
        .and_then(YamlValue::as_mapping)
        .context("generated CI workflow declares jobs")?;

    for job_id in &workspace_jobs {
        ensure!(
            job_ids.contains(job_id),
            "workspace member has a generated Rust job: {job_id}"
        );
    }

    let mut task_jobs = BTreeSet::new();
    for task in &task_configs {
        let id = task
            .get("id")
            .and_then(TomlValue::as_str)
            .context("workflow task has an ID")?;
        let emitted = task_job_ids(&job_ids, id);
        ensure!(
            !emitted.is_empty(),
            "configured task has a generated job: {id}"
        );
        task_jobs.extend(emitted);
    }

    let required = jobs
        .get(&YamlValue::String("required".to_owned()))
        .context("generated workflow has the Required gate")?;
    let required_needs = job_needs(required)?;
    for job_id in workspace_jobs.iter().chain(task_jobs.iter()) {
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
    let (workflow, job_ids) = workflow_jobs(&root)?;
    let jobs = yaml_field(&workflow, "jobs")
        .and_then(YamlValue::as_mapping)
        .context("generated CI workflow declares jobs")?;
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
        let emitted = task_job_ids(&job_ids, id);
        ensure!(!emitted.is_empty(), "verification task job exists: {id}");
        for job_id in emitted {
            let job = jobs
                .get(&YamlValue::String(job_id.clone()))
                .with_context(|| format!("verification job exists: {job_id}"))?;
            ensure!(
                yaml_field(job, "runs-on").and_then(YamlValue::as_str) == Some(expected_runner),
                "verification runner is bound to its declaration: {job_id}"
            );
            ensure!(
                job_needs(job)?.is_empty(),
                "verification job is independent and unconditional: {job_id}"
            );
            ensure!(
                yaml_field(job, "if").is_none(),
                "verification job is unconditional"
            );

            let permissions = yaml_field(job, "permissions")
                .and_then(YamlValue::as_mapping)
                .context("verification permissions are explicit")?;
            ensure!(
                permissions.get(&YamlValue::String("contents".to_owned()))
                    == Some(&YamlValue::String("read".to_owned())),
                "verification checkout has read-only contents permission"
            );
            ensure!(
                permissions.values().all(|permission| {
                    permission == &YamlValue::String("read".to_owned())
                        || permission == &YamlValue::String("none".to_owned())
                }),
                "verification job grants no write permission"
            );

            let steps = yaml_field(job, "steps")
                .and_then(YamlValue::as_sequence)
                .context("verification job has explicit steps")?;
            let checkout = steps.iter().find(|step| {
                yaml_field(step, "uses")
                    .and_then(YamlValue::as_str)
                    .is_some_and(|uses| uses.starts_with("actions/checkout@"))
            });
            let checkout = checkout.context("verification checks out its source")?;
            ensure!(
                yaml_field(checkout, "with")
                    .and_then(|with| yaml_field(with, "persist-credentials"))
                    .is_some_and(|value| value == &YamlValue::String("false".to_owned())),
                "verification checkout does not persist credentials"
            );
            ensure!(
                !steps.iter().any(|step| {
                    yaml_field(step, "uses")
                        .and_then(YamlValue::as_str)
                        .is_some_and(|uses| uses.contains("actions/cache"))
                }),
                "verification job has no shared tool cache"
            );
            let scripts = job_run_scripts(job);
            let invocation = format!("run --skip-tools {mise_task}");
            ensure!(
                scripts.iter().any(|script| script.contains(&invocation)),
                "verification runs only its declared Mise task: {job_id}"
            );
            ensure!(
                scripts.iter().any(|script| {
                    script.contains("ACTIONS_ID_TOKEN_REQUEST_TOKEN")
                        && script.contains("GITHUB_TOKEN")
                }),
                "verification task execution scrubs inherited credentials"
            );
            if id == "native-swiftlint" {
                ensure!(
                    scripts.iter().any(|script| {
                        script.contains("mise --no-env --locked --no-hooks install --jobs 2")
                            && script.contains("swiftlint")
                    }),
                    "SwiftLint is installed from its locked prebuilt tool identity"
                );
            }
        }
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
        boltffi.get("version").and_then(TomlValue::as_str) == Some("0.31.0"),
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
        ci_script.contains("mbx +1.99.0 xtask desktop")
            && !ci_script
                .lines()
                .any(|line| line.trim_start().starts_with("cargo ")),
        "native Rust commands route through the pinned MBX entry point"
    );

    let (workflow, job_ids) = workflow_jobs(&root)?;
    let emitted = task_job_ids(&job_ids, "native-desktop-ci");
    ensure!(
        emitted == ["task-native-desktop-ci"],
        "native build has one hosted job"
    );
    let jobs = yaml_field(&workflow, "jobs")
        .and_then(YamlValue::as_mapping)
        .context("generated CI workflow declares jobs")?;
    let job = jobs
        .get(&YamlValue::String(emitted[0].clone()))
        .context("native build job exists")?;
    ensure!(
        yaml_field(job, "runs-on").and_then(YamlValue::as_str) == Some("macos-26"),
        "native build job uses the configured ARM64 macOS 26 host"
    );
    ensure!(
        yaml_field(job, "timeout-minutes").and_then(YamlValue::as_i64) == Some(120),
        "native build retains its bounded timeout"
    );
    ensure!(
        yaml_field(job, "if").is_none(),
        "native build remains unconditional"
    );
    ensure!(
        job_needs(job)?.is_empty(),
        "native task has no unrelated dependencies"
    );
    let steps = yaml_field(job, "steps")
        .and_then(YamlValue::as_sequence)
        .context("native build job has explicit steps")?;
    let checkout = steps.iter().find(|step| {
        yaml_field(step, "uses")
            .and_then(YamlValue::as_str)
            .is_some_and(|uses| uses.starts_with("actions/checkout@"))
    });
    ensure!(
        checkout
            .and_then(|step| yaml_field(step, "with"))
            .and_then(|with| yaml_field(with, "persist-credentials"))
            .is_some_and(|value| value == &YamlValue::String("false".to_owned())),
        "native build checkout does not persist credentials"
    );
    let bootstrap = steps
        .iter()
        .find(|step| {
            yaml_field(step, "name").and_then(YamlValue::as_str)
                == Some("Install selected locked prebuilt tools")
        })
        .context("native job installs its selected locked tool closure")?;
    let bootstrap_scripts = yaml_field(bootstrap, "run")
        .map(yaml_strings)
        .unwrap_or_default();
    ensure!(
        bootstrap_scripts.iter().any(|script| {
            script.contains("mise --no-env --locked --no-hooks install --jobs 2")
        }),
        "native tool installation is lock-backed and bounded"
    );
    let source_guard = steps
        .iter()
        .find(|step| {
            yaml_field(step, "name").and_then(YamlValue::as_str)
                == Some("Verify locked MBX Rust route")
        })
        .context("native job verifies its pinned toolchain and source inputs")?;
    let guard_scripts = yaml_field(source_guard, "run")
        .map(yaml_strings)
        .unwrap_or_default();
    let guard_script = guard_scripts
        .iter()
        .find(|script| script.contains("shasum -a 256"))
        .context("native source guard checks input digests")?;
    for path in [
        "mise.toml",
        "mise.lock",
        "rust-toolchain.toml",
        "native/mise.toml",
        "native/mise.lock",
    ] {
        let digest = hex::encode(Sha256::digest(
            fs::read(root.join(path))
                .with_context(|| format!("native build source input exists: {path}"))?,
        ));
        ensure!(
            guard_script.contains(&format!("shasum -a 256 \"$workspace_root/{path}\""))
                && guard_script.contains(&format!("test \"$actual\" = '{digest}'")),
            "native source guard binds the exact current file digest: {path}"
        );
    }
    let scripts = job_run_scripts(job);
    ensure!(
        scripts
            .iter()
            .any(|script| script.contains("run --skip-tools ci"))
            && scripts.iter().any(|script| script.contains("MBX")),
        "native job verifies the MBX wrapper before running its declared task"
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
