use std::{
    fs,
    path::{Path, PathBuf},
};

use toml::Value;

use super::{CiArgs, e2e_selected, parse_capsule_export, validate_capsule_path};

#[test]
fn e2e_partition_selects_the_complete_docker_suite() {
    let args = CiArgs {
        fast: false,
        e2e: false,
        e2e_capsule: None,
        e2e_filter: None,
        base: "origin/main".to_owned(),
        only: vec!["e2e".to_owned()],
    };

    assert!(e2e_selected(&args));
}

#[test]
fn parse_capsule_export_accepts_single_quoted_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let capsule = temp.path().join("jackin-capsule");
    fs::write(&capsule, "").expect("capsule");

    let output = format!("export JACKIN_CAPSULE_BIN='{}'\n", capsule.display());

    assert_eq!(parse_capsule_export(&output).unwrap(), capsule);
}

#[test]
fn parse_capsule_export_rejects_missing_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let capsule = temp.path().join("missing-capsule");
    let output = format!("export JACKIN_CAPSULE_BIN='{}'\n", capsule.display());

    let err = parse_capsule_export(&output).unwrap_err().to_string();

    assert!(err.contains("capsule export path does not exist"));
}

#[test]
fn existing_relative_capsule_path_is_resolved_from_the_repository() {
    let temp = tempfile::tempdir().expect("tempdir");
    let capsule = temp.path().join("target/debug/jackin-capsule");
    fs::create_dir_all(capsule.parent().expect("parent")).expect("target directory");
    fs::write(&capsule, "").expect("capsule");

    assert_eq!(
        validate_capsule_path(temp.path(), Path::new("target/debug/jackin-capsule")).unwrap(),
        capsule
    );
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn workspace_file(path: &str) -> String {
    fs::read_to_string(workspace_root().join(path)).expect("workspace file")
}

const SCCACHE_ENV: [&str; 3] = ["CARGO_INCREMENTAL", "RUSTC_WRAPPER", "SCCACHE_GHA_ENABLED"];

fn assert_no_sccache_env(text: &str, context: &str) {
    for line in [
        "CARGO_INCREMENTAL: \"0\"",
        "RUSTC_WRAPPER: sccache",
        "SCCACHE_GHA_ENABLED: \"true\"",
        "CARGO_INCREMENTAL=0",
        "RUSTC_WRAPPER=sccache",
        "SCCACHE_GHA_ENABLED=true",
    ] {
        assert!(!text.contains(line), "{context} exports `{line}`");
    }
}

fn mise_task_block<'a>(mise: &'a str, task: &str) -> &'a str {
    let marker = format!("[tasks.{task}]");
    let start = mise
        .find(&marker)
        .unwrap_or_else(|| panic!("missing {marker}"));
    let body = &mise[start..];
    let end = body[marker.len()..]
        .find("\n[tasks.")
        .map_or(body.len(), |offset| marker.len() + offset);
    &body[..end]
}

fn assert_task_sccache_env(mise: &str, task: &str) {
    let block = mise_task_block(mise, task);
    for name in SCCACHE_ENV {
        assert!(
            block.contains(name),
            "task {task} does not scope {name} to its Cargo command"
        );
    }
}

fn config_array<'a>(config: &'a Value, key: &str) -> &'a [Value] {
    config
        .get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{key}"))
}

fn config_unit<'a>(units: &'a [Value], id: &str) -> &'a Value {
    units
        .iter()
        .find(|unit| unit.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("unit {id}"))
}

fn unit_has_tool(unit: &Value, tool: &str) -> bool {
    unit.get("mise_tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| tools.iter().any(|value| value.as_str() == Some(tool)))
}

fn assert_profile_contract(config: &Value) {
    let profiles = config
        .get("check_profile")
        .and_then(Value::as_array)
        .expect("check profiles");
    let mut sccache_profile_ids = Vec::new();
    for profile in profiles {
        let id = profile
            .get("id")
            .and_then(Value::as_str)
            .expect("check profile id");
        let has_sccache = profile
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| {
                tools
                    .iter()
                    .any(|tool| tool.as_str() == Some("cargo:sccache"))
            });
        if has_sccache {
            sccache_profile_ids.push(id);
        }
        let env = profile.get("env").and_then(Value::as_table);
        for name in SCCACHE_ENV {
            assert!(
                !env.is_some_and(|env| env.contains_key(name)),
                "check profile {id} exports {name} before installer setup"
            );
        }
    }
    assert_eq!(
        sccache_profile_ids,
        ["desktop-merge", "desktop-scheduled"],
        "all cargo:sccache check profiles must be wired"
    );
}

fn assert_swift_unit_contract(config: &Value) {
    let units = config_array(config, "units");
    let design_unit_id = "swift-package-native-design-prototypes-unifiedagentusage";
    let design_unit = config_unit(units, design_unit_id);
    assert!(
        !unit_has_tool(design_unit, "cargo:sccache"),
        "Swift member {design_unit_id} must not provision an unused wrapper"
    );
    for id in [
        "swift-package-native",
        "swift-xcodegen-native-project-yml-jackindesktop",
    ] {
        let unit = config_unit(units, id);
        assert!(
            unit_has_tool(unit, "cargo:sccache"),
            "Swift member {id} must provision its Cargo-backed task"
        );
        assert!(
            unit.get("env").is_none(),
            "Swift member {id} must keep wrapper env out of the generated job"
        );
    }
    assert!(
        design_unit.get("env").is_none(),
        "Swift member {design_unit_id} must keep wrapper env out of the generated job"
    );
}

fn assert_swift_caller_contract() {
    for workflow in [
        ".github/workflows/ci-main.yml",
        ".github/workflows/ci-pr.yml",
    ] {
        let text = workspace_file(workflow);
        for id in [
            "swift-package-native",
            "swift-xcodegen-native-project-yml-jackindesktop",
        ] {
            let block = text
                .split("\n  github-hosted-")
                .find(|block| block.contains(&format!("unit: {id}\n")))
                .unwrap_or_else(|| panic!("{workflow} caller for {id}"));
            assert!(
                block.contains("uses: ./.github/workflows/ci-unit-swift.yml"),
                "{workflow} caller for {id} uses the wrong reusable workflow"
            );
            assert!(
                block.contains("mise_tools:") && block.contains("cargo:sccache"),
                "{workflow} caller for {id} does not provision sccache"
            );
        }
        let design_block = text
            .split("\n  github-hosted-")
            .find(|block| {
                block.contains("unit: swift-package-native-design-prototypes-unifiedagentusage\n")
            })
            .unwrap_or_else(|| panic!("{workflow} caller for swift design unit"));
        assert_no_sccache_env(design_block, &format!("{workflow} Swift design caller"));
    }
}

fn assert_generated_workflow_contract() {
    let swift = workspace_file(".github/workflows/ci-unit-swift.yml");
    assert_no_sccache_env(&swift, "Swift reusable job");
    assert!(
        swift.find("- name: Set up Mise tools") < swift.find("- name: Run unit checks"),
        "Swift wrapper must be installed before checks"
    );

    for workflow in [
        ".github/workflows/desktop-merge.yml",
        ".github/workflows/desktop-scheduled.yml",
    ] {
        let text = workspace_file(workflow);
        assert_no_sccache_env(&text, workflow);
        assert!(
            text.find("install_args:").expect("Mise setup")
                < text.find("Run desktop-").expect("desktop task"),
            "{workflow} must install sccache before its Cargo task"
        );
    }

    for workflow in [
        ".github/workflows/ci-unit-rust.yml",
        ".github/workflows/release.yml",
    ] {
        let text = workspace_file(workflow);
        assert_no_sccache_env(&text, workflow);
    }
}

fn assert_mise_task_contract() {
    let mise = workspace_file("mise.toml");
    for task in ["desktop-xcframework", "desktop-ci"] {
        assert_task_sccache_env(&mise, task);
    }
    for task in [
        "desktop-build",
        "desktop-verify",
        "desktop-release-tools",
        "desktop-release-env",
    ] {
        assert_no_sccache_env(mise_task_block(&mise, task), &format!("mise task {task}"));
    }
}

#[test]
fn sccache_bootstrap_is_task_scoped_after_tool_install() {
    let config: Value = toml::from_str(&workspace_file(".github-gen/velnor-workflow.toml"))
        .expect("parse Velnor workflow config");
    assert_profile_contract(&config);
    assert_swift_unit_contract(&config);
    assert_swift_caller_contract();
    assert_generated_workflow_contract();
    assert_mise_task_contract();
}

#[test]
fn sccache_is_absent_from_installer_environment() {
    for workflow in [
        ".github/workflows/ci-unit-swift.yml",
        ".github/workflows/desktop-merge.yml",
        ".github/workflows/desktop-scheduled.yml",
    ] {
        let text = workspace_file(workflow);
        let setup = text
            .find("- name: Set up Mise")
            .unwrap_or_else(|| panic!("{workflow} has no Mise installer"));
        assert_no_sccache_env(&text[..setup], &format!("{workflow} installer prefix"));
    }
}

#[test]
fn desktop_merge_declaration_covers_push_and_pull_request() {
    let config: Value = toml::from_str(&workspace_file(".github-gen/velnor-workflow.toml"))
        .expect("parse Velnor workflow config");
    let declarations = config
        .get("declare")
        .and_then(Value::as_array)
        .expect("declarations");
    let desktop_merge = declarations
        .iter()
        .filter(|declaration| {
            declaration.get("primitive").and_then(Value::as_str) == Some("scheduled-checks")
                && declaration.get("file").and_then(Value::as_str) == Some("desktop-merge.yml")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        desktop_merge.len(),
        1,
        "desktop merge must have one declaration"
    );

    let args = desktop_merge[0]
        .get("args")
        .and_then(Value::as_table)
        .expect("desktop merge declaration args");
    let events = args
        .get("events")
        .and_then(Value::as_array)
        .expect("desktop merge events")
        .iter()
        .map(|event| event.as_str().expect("event name"))
        .collect::<Vec<_>>();
    assert_eq!(events, ["push", "pull_request"]);
    assert_eq!(
        args.get("branches")
            .and_then(Value::as_array)
            .expect("desktop merge push branches")
            .iter()
            .map(|branch| branch.as_str().expect("branch name"))
            .collect::<Vec<_>>(),
        ["main"]
    );
    assert_eq!(
        args.get("profiles")
            .and_then(Value::as_array)
            .expect("desktop merge profiles")
            .iter()
            .map(|profile| profile.as_str().expect("profile id"))
            .collect::<Vec<_>>(),
        ["desktop-merge"]
    );
}

#[test]
fn desktop_merge_and_scheduled_contracts_preserve_cadence_concurrency_and_gates() {
    let merge = workspace_file(".github/workflows/desktop-merge.yml");
    assert!(
        merge.contains("on:\n  push:\n    branches: [main]\n  pull_request:\n  workflow_dispatch:")
    );
    assert!(!merge.contains("  schedule:"));
    assert!(merge.contains("group: desktop-merge-${{ github.repository }}-${{ github.ref }}"));
    assert!(merge.contains("cancel-in-progress: ${{ github.event_name == 'pull_request' }}"));
    assert_eq!(
        merge.matches("run: mise run desktop-merge").count(),
        1,
        "desktop merge must have one generated task caller"
    );
    let generated_workflow_dir = workspace_root().join(".github/workflows");
    let direct_callers = fs::read_dir(generated_workflow_dir)
        .expect("generated workflow directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("yml"))
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter(|workflow| workflow.contains("run: mise run desktop-merge"))
        .count();
    assert_eq!(
        direct_callers, 1,
        "desktop merge must have one generated workflow caller"
    );

    let scheduled = workspace_file(".github/workflows/desktop-scheduled.yml");
    assert!(scheduled.contains("  schedule:\n    - cron: \"41 4 * * 1\""));
    assert!(
        scheduled.contains("group: desktop-scheduled-${{ github.repository }}-${{ github.ref }}")
    );
    assert!(scheduled.contains("cancel-in-progress: true"));
    assert!(!scheduled.contains("  pull_request:"));

    for workflow in [
        ".github/workflows/ci-main.yml",
        ".github/workflows/ci-pr.yml",
    ] {
        let text = workspace_file(workflow);
        assert!(
            text.contains("plan_digest"),
            "{workflow} lost plan digest wiring"
        );
    }

    let swift = workspace_file(".github/workflows/ci-unit-swift.yml");
    assert!(swift.contains("product_transport_ready:"));
    assert!(swift.contains("SELECTION_PLAN_DIGEST:"));

    let mise = workspace_file("mise.toml");
    for required in [
        "[tasks.desktop-ci]",
        "mise run desktop-bindings-check",
        "mise run desktop-generate",
        "mise run desktop-format-check",
        "mise run desktop-lint",
        "mise run desktop-test",
        "mise run desktop-build",
        "cargo xtask desktop test-swift",
        "mise run desktop-verify",
        "[tasks.desktop-merge]",
        "mise run desktop-ci",
        "mise run desktop-test-ui",
    ] {
        assert!(mise.contains(required), "desktop graph lost `{required}`");
    }
}
