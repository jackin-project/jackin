use anyhow::{Context, Result, ensure};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use toml::Value as TomlValue;

const ARCHITECT_REPOSITORY: &str = "jackin-project/jackin-the-architect";
const ARCHITECT_COMMIT: &str = "7db69b62f598a0971809ee4a006ad3f5477d0996";
const ARCHITECT_MANIFEST_SHA256: &str =
    "b38e506587c98137d0a1a88247fb68afc9f9f215c8104c838df251933a917ae0";
const REQUIRED_VERIFICATION_TASKS: [(&str, &str, &str, &str); 3] = [
    (
        "native-swift-format",
        "desktop-format-check",
        "macos-arm64",
        "swift-format lint",
    ),
    (
        "native-swiftlint",
        "desktop-lint",
        "macos-arm64",
        "swiftlint lint --strict",
    ),
    (
        "construct-upstream-assets",
        "renovate-upstream-sources",
        "linux-x64",
        "docker/construct/versions.env",
    ),
];
const DENIED_CREDENTIALS_ARGV_PREFIX: &str = "env -u MISE_GITHUB_TOKEN -u GITHUB_TOKEN -u GH_TOKEN \
    -u ACTIONS_RUNTIME_TOKEN -u ACTIONS_ID_TOKEN_REQUEST_TOKEN \
    -u ACTIONS_ID_TOKEN_REQUEST_URL -u CARGO_REGISTRY_TOKEN -u NPM_TOKEN \
    -u NODE_AUTH_TOKEN";

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_toml(path: &Path) -> Result<TomlValue> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&contents).with_context(|| format!("parsing {}", path.display()))
}

fn core_string_constant<'a>(contents: &'a str, name: &str) -> Result<&'a str> {
    let marker = format!("pub const {name}: &str = \"");
    contents
        .split_once(marker.as_str())
        .and_then(|(_, rest)| rest.split_once('"').map(|(value, _)| value))
        .with_context(|| format!("Jackin declares {name}"))
}

fn verification_tasks(root: &Path) -> Result<BTreeMap<String, TomlValue>> {
    let config = read_toml(&root.join(".velnor/config.toml"))?;
    let tasks = config
        .get("workflow")
        .and_then(|value| value.get("tasks"))
        .and_then(TomlValue::as_array)
        .context("workflow.tasks declares maintained verification jobs")?;
    let mut by_id = BTreeMap::new();
    for task in tasks {
        let kind = task
            .get("kind")
            .and_then(TomlValue::as_str)
            .context("workflow task kind is explicit")?;
        ensure!(
            kind == "verification",
            "only closed verification tasks belong here"
        );
        let id = task
            .get("id")
            .and_then(TomlValue::as_str)
            .context("workflow task id is stable")?;
        ensure!(
            by_id.insert(id.to_owned(), task.clone()).is_none(),
            "duplicate task id {id}"
        );
    }
    Ok(by_id)
}

fn workspace_crate_job_ids(root: &Path) -> Result<BTreeSet<String>> {
    let manifest = read_toml(&root.join("Cargo.toml"))?;
    let members = manifest
        .get("workspace")
        .and_then(|value| value.get("members"))
        .and_then(TomlValue::as_array)
        .context("workspace members are declared")?;

    members
        .iter()
        .try_fold(BTreeSet::new(), |mut jobs, member| {
            let member = member
                .as_str()
                .context("workspace member path is a string")?;
            let member_manifest = read_toml(&root.join(member).join("Cargo.toml"))?;
            let name = member_manifest
                .get("package")
                .and_then(|value| value.get("name"))
                .and_then(TomlValue::as_str)
                .context("workspace crate declares its package name")?;
            jobs.insert(format!("rust-{name}"));
            Ok(jobs)
        })
}

fn generated_workflows(root: &Path) -> Result<Vec<(PathBuf, JsonValue)>> {
    let directory = root.join(".github/workflows");
    let entries =
        fs::read_dir(&directory).with_context(|| format!("reading {}", directory.display()))?;
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .context("reading workflow entries")?;
    paths.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == "yml" || extension == "yaml")
    });
    paths.sort();
    ensure!(!paths.is_empty(), "generated workflow directory is empty");
    paths
        .into_iter()
        .map(|path| {
            let contents =
                fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            let workflow: JsonValue = serde_yaml_ng::from_str(&contents)
                .with_context(|| format!("parsing {}", path.display()))?;
            Ok((path, workflow))
        })
        .collect()
}

fn workflow_jobs<'a>(
    path: &Path,
    workflow: &'a JsonValue,
) -> Result<&'a serde_json::Map<String, JsonValue>> {
    workflow
        .get("jobs")
        .and_then(JsonValue::as_object)
        .with_context(|| format!("{} has no jobs mapping", path.display()))
}

fn required_needs(job: &JsonValue, label: &str) -> Result<BTreeSet<String>> {
    let value = job
        .get("needs")
        .with_context(|| format!("{label} job has no required fan-in"))?;
    let list = value
        .as_array()
        .with_context(|| format!("{label} job needs must be a sequence"))?;
    list.iter()
        .map(|entry| {
            entry
                .as_str()
                .with_context(|| format!("{label} job need is not a string"))
                .map(str::to_owned)
        })
        .collect()
}

fn action_uses_is_sha_pinned(uses: &str, action: &str) -> bool {
    uses.starts_with(action)
        && uses.rsplit_once('@').is_some_and(|(_, sha)| {
            sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

fn assert_credential_env_is_empty(value: &JsonValue, label: &str) -> Result<()> {
    const CREDENTIAL_ENV: [&str; 9] = [
        "ACTIONS_ID_TOKEN_REQUEST_TOKEN",
        "ACTIONS_ID_TOKEN_REQUEST_URL",
        "ACTIONS_RUNTIME_TOKEN",
        "CARGO_REGISTRY_TOKEN",
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "MISE_GITHUB_TOKEN",
        "NODE_AUTH_TOKEN",
        "NPM_TOKEN",
    ];

    match value {
        JsonValue::Object(object) => {
            if let Some(environment) = object.get("env").and_then(JsonValue::as_object) {
                for key in CREDENTIAL_ENV {
                    if let Some(value) = environment.get(key) {
                        ensure!(
                            value.as_str() == Some(""),
                            "{label} exposes credential environment variable {key}"
                        );
                    }
                }
            }
            for nested in object.values() {
                assert_credential_env_is_empty(nested, label)?;
            }
        }
        JsonValue::Array(array) => {
            for nested in array {
                assert_credential_env_is_empty(nested, label)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[test]
fn architect_manifest_snapshot_is_bound_to_an_immutable_source_commit() -> Result<()> {
    let root = repository_root();
    let fixture = root.join("crates/jackin-xtask/tests/fixtures/architect");
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
    let core_constants = fs::read_to_string(root.join("crates/jackin-core/src/constants.rs"))
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

#[test]
fn required_fan_in_covers_every_workspace_crate_and_configured_verification_task() -> Result<()> {
    let root = repository_root();
    let configured_tasks = verification_tasks(&root)?;
    ensure!(
        configured_tasks.len() == REQUIRED_VERIFICATION_TASKS.len(),
        "verification task inventory changed; update the reviewed contract"
    );

    let mut expected = workspace_crate_job_ids(&root)?;
    for crate_name in ["jackin-manifest", "jackin-xtask"] {
        let job_id = format!("rust-{crate_name}");
        ensure!(
            expected.contains(&job_id),
            "{job_id} must remain in Required to run the Architect parser and provenance contracts"
        );
    }
    let mise = read_toml(&root.join("mise.toml"))?;
    let mise_tasks = mise
        .get("tasks")
        .and_then(TomlValue::as_table)
        .context("Mise tasks are declared")?;
    for (id, task_name, runner, command_marker) in REQUIRED_VERIFICATION_TASKS {
        let task = configured_tasks
            .get(id)
            .with_context(|| format!("required verification task {id} is absent"))?;
        ensure!(
            task.get("mise_task").and_then(TomlValue::as_str) == Some(task_name),
            "verification task {id} changed its Mise body"
        );
        ensure!(
            task.get("runner").and_then(TomlValue::as_str) == Some(runner),
            "verification task {id} changed its runner"
        );
        ensure!(
            task.get("timeout_minutes").and_then(TomlValue::as_integer) == Some(10),
            "verification task {id} changed its timeout"
        );

        let mise_task = mise_tasks
            .get(task_name)
            .with_context(|| format!("Mise task {task_name} is absent"))?;
        let body = mise_task
            .get("run")
            .and_then(TomlValue::as_str)
            .with_context(|| format!("Mise task {task_name} has no single run body"))?;
        ensure!(
            body.contains(command_marker),
            "{task_name} lost its maintained check"
        );
        for rust_command in ["cargo", "rustc", "rustup", "mbx"] {
            ensure!(
                !body.contains(rust_command),
                "verification task {task_name} may not compile Rust"
            );
        }
        expected.insert(format!("task-{id}"));
    }

    let workflows = generated_workflows(&root)?;
    let mut found_required = false;
    for (path, workflow) in &workflows {
        let jobs = workflow_jobs(path, workflow)?;
        let Some(required) = jobs.get("required") else {
            continue;
        };
        found_required = true;
        let actual = required_needs(required, "required")?;
        for job_id in &expected {
            ensure!(
                jobs.contains_key(job_id),
                "{} is missing required job {job_id}",
                path.display()
            );
            ensure!(
                actual.contains(job_id),
                "{} Required.needs omits {job_id}",
                path.display()
            );
        }
    }
    ensure!(
        found_required,
        "generated workflows have no Required fan-in job"
    );
    Ok(())
}

#[test]
fn configured_verification_jobs_are_isolated_and_run_only_the_declared_mise_task() -> Result<()> {
    let root = repository_root();
    let configured_tasks = verification_tasks(&root)?;
    let workflows = generated_workflows(&root)?;
    let mut found_jobs = BTreeSet::new();
    for (path, workflow) in &workflows {
        let jobs = workflow_jobs(path, workflow)?;
        for (id, task_name, runner, _) in REQUIRED_VERIFICATION_TASKS {
            let job_id = format!("task-{id}");
            let Some(job) = jobs.get(&job_id) else {
                continue;
            };
            found_jobs.insert(job_id.clone());
            ensure!(
                configured_tasks
                    .get(id)
                    .and_then(|task| task.get("mise_task"))
                    .and_then(TomlValue::as_str)
                    == Some(task_name),
                "{job_id} no longer matches its configured Mise task"
            );
            let expected_runner = match runner {
                "macos-arm64" => "macos-15",
                "linux-x64" => "ubuntu-26.04",
                _ => anyhow::bail!("unreviewed runner {runner}"),
            };
            ensure!(
                job.get("runs-on").and_then(JsonValue::as_str) == Some(expected_runner),
                "{job_id} changed its runner"
            );
            ensure!(
                job.get("timeout-minutes").and_then(JsonValue::as_i64) == Some(10),
                "{job_id} changed its timeout"
            );
            ensure!(job.get("needs").is_none(), "{job_id} must be standalone");
            let permissions = job
                .get("permissions")
                .and_then(JsonValue::as_object)
                .with_context(|| format!("{job_id} must declare job permissions"))?;
            ensure!(permissions.len() == 4, "{job_id} permissions changed");
            ensure!(
                permissions.get("contents").and_then(JsonValue::as_str) == Some("read"),
                "{job_id} must have contents: read"
            );
            for scope in ["actions", "pull-requests", "id-token"] {
                ensure!(
                    permissions.get(scope).and_then(JsonValue::as_str) == Some("none"),
                    "{job_id} must set {scope}: none"
                );
            }

            let serialized = serde_json::to_string(job).context("job serializes to JSON")?;
            let lowercase = serialized.to_ascii_lowercase();
            for forbidden in ["actions/cache", "secrets.", "github.token"] {
                ensure!(
                    !lowercase.contains(forbidden),
                    "{job_id} must not use cache or credential inputs: {forbidden}"
                );
            }
            assert_credential_env_is_empty(job, &job_id)?;

            let steps = job
                .get("steps")
                .and_then(JsonValue::as_array)
                .with_context(|| format!("{job_id} has no steps"))?;
            let mut checkout_is_credential_free = false;
            let mut checkout_action_count = 0;
            let mut mise_cache_is_disabled = false;
            let mut mise_action_count = 0;
            let mut locked_install_count = 0;
            let mut declared_task_count = 0;
            let mut run_step_count = 0;
            for step in steps {
                let uses = step
                    .get("uses")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default();
                let inputs = step.get("with").and_then(JsonValue::as_object);
                if uses.starts_with("actions/checkout@") {
                    checkout_action_count += 1;
                    ensure!(
                        action_uses_is_sha_pinned(uses, "actions/checkout@"),
                        "{job_id} checkout action must be SHA-pinned"
                    );
                    checkout_is_credential_free = inputs
                        .and_then(|inputs| inputs.get("persist-credentials"))
                        .is_some_and(|value| {
                            value.as_bool() == Some(false) || value.as_str() == Some("false")
                        });
                }
                if uses.starts_with("jdx/mise-action@") {
                    mise_action_count += 1;
                    ensure!(
                        action_uses_is_sha_pinned(uses, "jdx/mise-action@"),
                        "{job_id} Mise action must be SHA-pinned"
                    );
                    let inputs =
                        inputs.with_context(|| format!("{job_id} Mise action has no inputs"))?;
                    ensure!(inputs.len() == 6, "{job_id} Mise action inputs changed");
                    let version = inputs
                        .get("version")
                        .and_then(JsonValue::as_str)
                        .context("Mise action version is pinned")?;
                    let sha256 = inputs
                        .get("sha256")
                        .and_then(JsonValue::as_str)
                        .context("Mise action archive digest is pinned")?;
                    ensure!(!version.is_empty(), "{job_id} Mise version cannot be empty");
                    ensure!(
                        sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
                        "{job_id} Mise action SHA-256 must be pinned"
                    );
                    mise_cache_is_disabled =
                        ["cache", "cache_save", "env", "install"].iter().all(|key| {
                            inputs.get(*key).is_some_and(|value| {
                                value.as_bool() == Some(false) || value.as_str() == Some("false")
                            })
                        });
                }
                if let Some(run) = step.get("run").and_then(JsonValue::as_str) {
                    run_step_count += 1;
                    ensure!(
                        run.starts_with(DENIED_CREDENTIALS_ARGV_PREFIX),
                        "{job_id} run step must remove every inherited credential"
                    );
                    if run.contains("mise install --locked") {
                        locked_install_count += 1;
                    }
                    if run.contains(&format!("mise run {task_name}")) {
                        declared_task_count += 1;
                    }
                }
            }
            ensure!(
                checkout_is_credential_free,
                "{job_id} must disable checkout credentials"
            );
            ensure!(
                checkout_action_count == 1,
                "{job_id} must have one checkout action"
            );
            ensure!(mise_cache_is_disabled, "{job_id} must disable Mise caching");
            ensure!(mise_action_count == 1, "{job_id} must have one Mise action");
            ensure!(
                steps.len() == 4,
                "{job_id} gained an unreviewed action or step"
            );
            ensure!(
                locked_install_count == 1,
                "{job_id} must install locked Mise tools once"
            );
            ensure!(
                run_step_count == 2,
                "{job_id} gained an unreviewed shell step"
            );
            ensure!(
                declared_task_count == 1,
                "{job_id} must run its declared Mise task once"
            );
        }
    }
    for (id, _, _, _) in REQUIRED_VERIFICATION_TASKS {
        ensure!(
            found_jobs.contains(&format!("task-{id}")),
            "generated workflow graph omits task-{id}"
        );
    }
    Ok(())
}
