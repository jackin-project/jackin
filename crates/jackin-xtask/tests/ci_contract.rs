#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "maintained CI contract tests must fail with the missing declaration"
)]

use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use serde_json::Value as JsonValue;
use toml::Value as TomlValue;

const ARCHITECT_REPOSITORY: &str = "jackin-project/jackin-the-architect";
const ARCHITECT_COMMIT: &str = "0592d0deeaeaa5b785fa67a43d23d3b627552720";
const ARCHITECT_MANIFEST_SHA256: &str =
    "eb08cf89aa32971c17db9875ec633ac22fe609927abf23fd18182756819e7fca";
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

fn repository_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_toml(path: &Path) -> TomlValue {
    let contents = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
    toml::from_str(&contents)
        .unwrap_or_else(|error| panic!("parsing {}: {error}", path.display()))
}

fn verification_tasks(root: &Path) -> BTreeMap<String, TomlValue> {
    let config = read_toml(&root.join(".velnor/config.toml"));
    let tasks = config
        .get("workflow")
        .and_then(|value| value.get("tasks"))
        .and_then(TomlValue::as_array)
        .expect("workflow.tasks declares maintained verification jobs");
    let mut by_id = BTreeMap::new();
    for task in tasks {
        let kind = task
            .get("kind")
            .and_then(TomlValue::as_str)
            .expect("workflow task kind is explicit");
        assert_eq!(kind, "verification", "only closed verification tasks belong here");
        let id = task
            .get("id")
            .and_then(TomlValue::as_str)
            .expect("workflow task id is stable");
        assert!(by_id.insert(id.to_owned(), task.clone()).is_none(), "duplicate task id {id}");
    }
    by_id
}

fn workspace_crate_job_ids(root: &Path) -> BTreeSet<String> {
    let manifest = read_toml(&root.join("Cargo.toml"));
    let members = manifest
        .get("workspace")
        .and_then(|value| value.get("members"))
        .and_then(TomlValue::as_array)
        .expect("workspace members are declared");

    members
        .iter()
        .map(|member| {
            let member = member
                .as_str()
                .expect("workspace member path is a string");
            let member_manifest = read_toml(&root.join(member).join("Cargo.toml"));
            let name = member_manifest
                .get("package")
                .and_then(|value| value.get("name"))
                .and_then(TomlValue::as_str)
                .expect("workspace crate declares its package name");
            format!("rust-{name}")
        })
        .collect()
}

fn generated_workflows(root: &Path) -> Vec<(PathBuf, JsonValue)> {
    let directory = root.join(".github/workflows");
    let entries = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("reading {}: {error}", directory.display()));
    let mut paths = entries
        .map(|entry| {
            entry
                .unwrap_or_else(|error| panic!("reading workflow entry: {error}"))
                .path()
        })
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yml" || extension == "yaml")
        })
        .collect::<Vec<_>>();
    paths.sort();
    assert!(!paths.is_empty(), "generated workflow directory is empty");
    paths
        .into_iter()
        .map(|path| {
            let contents = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
            let workflow: JsonValue = serde_yaml_ng::from_str(&contents)
                .unwrap_or_else(|error| panic!("parsing {}: {error}", path.display()));
            (path, workflow)
        })
        .collect()
}

fn needs(job: &JsonValue, label: &str) -> BTreeSet<String> {
    let value = job
        .get("needs")
        .unwrap_or_else(|| panic!("{label} job has no required fan-in"));
    let list = value
        .as_array()
        .unwrap_or_else(|| panic!("{label} job needs must be a sequence"));
    list.iter()
        .map(|entry| {
            entry
                .as_str()
                .unwrap_or_else(|| panic!("{label} job need is not a string"))
                .to_owned()
        })
        .collect()
}

fn assert_credential_env_is_empty(value: &JsonValue, label: &str) {
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
                        assert_eq!(
                            value.as_str(),
                            Some(""),
                            "{label} exposes credential environment variable {key}"
                        );
                    }
                }
            }
            for nested in object.values() {
                assert_credential_env_is_empty(nested, label);
            }
        }
        JsonValue::Array(array) => {
            for nested in array {
                assert_credential_env_is_empty(nested, label);
            }
        }
        _ => {}
    }
}

#[test]
fn architect_manifest_snapshot_is_bound_to_an_immutable_source_commit() {
    let root = repository_root();
    let fixture = root.join("crates/jackin-xtask/tests/fixtures/architect");
    let provenance: TomlValue = toml::from_str(
        &fs::read_to_string(fixture.join("provenance.toml")).expect("role provenance exists"),
    )
    .expect("role provenance is valid TOML");
    let repository = provenance
        .get("repository")
        .and_then(TomlValue::as_str)
        .expect("role repository is pinned");
    let commit = provenance
        .get("commit")
        .and_then(TomlValue::as_str)
        .expect("role commit is pinned");
    let source_path = provenance
        .get("path")
        .and_then(TomlValue::as_str)
        .expect("role manifest path is pinned");
    let expected_sha256 = provenance
        .get("sha256")
        .and_then(TomlValue::as_str)
        .expect("role manifest digest is pinned");
    let source_url = provenance
        .get("url")
        .and_then(TomlValue::as_str)
        .expect("immutable source URL is pinned");

    assert_eq!(repository, ARCHITECT_REPOSITORY);
    assert_eq!(commit, ARCHITECT_COMMIT);
    assert_eq!(source_path, "jackin.role.toml");
    assert_eq!(expected_sha256, ARCHITECT_MANIFEST_SHA256);
    assert_eq!(
        source_url,
        format!(
            "https://github.com/{repository}/blob/{commit}/{source_path}"
        )
    );
    assert_eq!(commit.len(), 40, "source revision must be a full Git commit ID");
    assert!(commit.bytes().all(|byte| byte.is_ascii_hexdigit()));

    let manifest = fs::read(fixture.join(source_path)).expect("pinned role manifest exists");
    let actual_sha256 = hex::encode(Sha256::digest(&manifest));
    assert_eq!(actual_sha256, expected_sha256, "pinned role content changed");

    let manifest_text = std::str::from_utf8(&manifest).expect("pinned role manifest is UTF-8");
    let manifest: TomlValue =
        toml::from_str(manifest_text).expect("pinned role manifest is valid TOML");
    let manifest_version = manifest
        .get("version")
        .and_then(TomlValue::as_str)
        .expect("role manifest declares its version");
    let constants = fs::read_to_string(root.join("crates/jackin-core/src/constants.rs"))
        .expect("Jackin manifest-version declaration exists");
    let version_marker = "pub const CURRENT_MANIFEST_VERSION: &str = \"";
    let current_version = constants
        .split_once(version_marker)
        .and_then(|(_, rest)| rest.split_once('\"').map(|(version, _)| version))
        .expect("Jackin declares CURRENT_MANIFEST_VERSION");
    assert_eq!(manifest_version, current_version);

    let agents = manifest
        .get("agents")
        .and_then(TomlValue::as_array)
        .expect("Architect declares its supported agents");
    let actual_agents = agents
        .iter()
        .map(TomlValue::as_str)
        .collect::<Option<Vec<_>>>()
        .expect("agent names are strings");
    assert_eq!(
        actual_agents,
        ["claude", "codex", "amp", "opencode", "kimi", "grok"]
    );
}

#[test]
fn required_fan_in_covers_every_workspace_crate_and_configured_verification_task() {
    let root = repository_root();
    let configured_tasks = verification_tasks(&root);
    assert_eq!(
        configured_tasks.len(),
        REQUIRED_VERIFICATION_TASKS.len(),
        "verification task inventory changed; update the reviewed contract"
    );

    let mut expected = workspace_crate_job_ids(&root);
    let mise = read_toml(&root.join("mise.toml"));
    let mise_tasks = mise
        .get("tasks")
        .and_then(TomlValue::as_table)
        .expect("Mise tasks are declared");
    for (id, task_name, runner, command_marker) in REQUIRED_VERIFICATION_TASKS {
        let task = configured_tasks
            .get(id)
            .unwrap_or_else(|| panic!("required verification task {id} is absent"));
        assert_eq!(
            task.get("mise_task").and_then(TomlValue::as_str),
            Some(task_name)
        );
        assert_eq!(
            task.get("runner").and_then(TomlValue::as_str),
            Some(runner)
        );
        assert_eq!(
            task.get("timeout_minutes").and_then(TomlValue::as_integer),
            Some(10)
        );

        let mise_task = mise_tasks
            .get(task_name)
            .unwrap_or_else(|| panic!("Mise task {task_name} is absent"));
        let body = mise_task
            .get("run")
            .and_then(TomlValue::as_str)
            .unwrap_or_else(|| panic!("Mise task {task_name} has no single run body"));
        assert!(body.contains(command_marker), "{task_name} lost its maintained check");
        for rust_command in ["cargo", "rustc", "rustup", "mbx"] {
            assert!(
                !body.contains(rust_command),
                "verification task {task_name} may not compile Rust"
            );
        }
        expected.insert(format!("task-{id}"));
    }

    let workflows = generated_workflows(&root);
    let mut found_required = false;
    for (path, workflow) in &workflows {
        let jobs = workflow
            .get("jobs")
            .and_then(JsonValue::as_object)
            .unwrap_or_else(|| panic!("{} has no jobs mapping", path.display()));
        let Some(required) = jobs.get("required") else {
            continue;
        };
        found_required = true;
        let actual = needs(required, "required");
        for job_id in &expected {
            assert!(
                jobs.contains_key(job_id),
                "{} is missing required job {job_id}",
                path.display()
            );
            assert!(
                actual.contains(job_id),
                "{} Required.needs omits {job_id}",
                path.display()
            );
        }
    }
    assert!(found_required, "generated workflows have no Required fan-in job");
}

#[test]
fn configured_verification_jobs_are_isolated_and_run_only_the_declared_mise_task() {
    let root = repository_root();
    let configured_tasks = verification_tasks(&root);
    let workflows = generated_workflows(&root);
    for (path, workflow) in &workflows {
        let jobs = workflow
            .get("jobs")
            .and_then(JsonValue::as_object)
            .unwrap_or_else(|| panic!("{} has no jobs mapping", path.display()));
        for (id, task_name, runner, _) in REQUIRED_VERIFICATION_TASKS {
            let job_id = format!("task-{id}");
            let Some(job) = jobs.get(&job_id) else {
                continue;
            };
            assert_eq!(
                configured_tasks
                    .get(id)
                    .and_then(|task| task.get("mise_task"))
                    .and_then(TomlValue::as_str),
                Some(task_name)
            );
            let expected_runner = match runner {
                "macos-arm64" => "macos-15",
                "linux-x64" => "ubuntu-26.04",
                _ => panic!("unreviewed runner {runner}"),
            };
            assert_eq!(
                job.get("runs-on").and_then(JsonValue::as_str),
                Some(expected_runner)
            );
            assert_eq!(
                job.get("timeout-minutes").and_then(JsonValue::as_integer),
                Some(10)
            );
            assert!(job.get("needs").is_none(), "{job_id} must be a standalone job");
            let permissions = job
                .get("permissions")
                .and_then(JsonValue::as_object)
                .unwrap_or_else(|| panic!("{job_id} must declare job permissions"));
            assert_eq!(permissions.len(), 4, "{job_id} permissions changed");
            assert_eq!(permissions.get("contents").and_then(JsonValue::as_str), Some("read"));
            for scope in ["actions", "pull-requests", "id-token"] {
                assert_eq!(permissions.get(scope).and_then(JsonValue::as_str), Some("none"));
            }

            let serialized = serde_json::to_string(job).expect("job serializes to JSON");
            let lowercase = serialized.to_ascii_lowercase();
            for forbidden in ["actions/cache", "secrets.", "github.token"] {
                assert!(
                    !lowercase.contains(forbidden),
                    "{job_id} must not use cache or credential inputs: {forbidden}"
                );
            }
            assert_credential_env_is_empty(job, &job_id);

            let steps = job
                .get("steps")
                .and_then(JsonValue::as_array)
                .unwrap_or_else(|| panic!("{job_id} has no steps"));
            let mut checkout_is_credential_free = false;
            let mut mise_cache_is_disabled = false;
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
                    checkout_is_credential_free = inputs
                        .and_then(|inputs| inputs.get("persist-credentials"))
                        .is_some_and(|value| {
                            value.as_bool() == Some(false)
                                || value.as_str() == Some("false")
                        });
                }
                if uses.starts_with("jdx/mise-action@") {
                    mise_cache_is_disabled = inputs.is_some_and(|inputs| {
                        ["cache", "cache_save"].iter().all(|key| {
                            inputs.get(*key).is_some_and(|value| {
                                value.as_bool() == Some(false)
                                    || value.as_str() == Some("false")
                            })
                        })
                    });
                }
                if let Some(run) = step.get("run").and_then(JsonValue::as_str) {
                    run_step_count += 1;
                    if run.contains("mise install --locked") {
                        locked_install_count += 1;
                    }
                    if run.contains(&format!("mise run {task_name}")) {
                        declared_task_count += 1;
                    }
                }
            }
            assert!(checkout_is_credential_free, "{job_id} must disable checkout credentials");
            assert!(mise_cache_is_disabled, "{job_id} must disable Mise caching");
            assert_eq!(locked_install_count, 1, "{job_id} must install locked Mise tools once");
            assert_eq!(run_step_count, 2, "{job_id} gained an unreviewed shell step");
            assert_eq!(declared_task_count, 1, "{job_id} must run its declared Mise task once");
        }
    }
    for (id, _, _, _) in REQUIRED_VERIFICATION_TASKS {
        assert!(
            workflows.iter().any(|(_, workflow)| {
                workflow
                    .get("jobs")
                    .and_then(JsonValue::as_object)
                    .is_some_and(|jobs| jobs.contains_key(&format!("task-{id}")))
            }),
            "generated workflow graph omits task-{id}"
        );
    }
}
