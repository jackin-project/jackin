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

fn assert_sccache_env(value: &Value, context: &str) {
    let env = value
        .get("env")
        .and_then(Value::as_table)
        .unwrap_or_else(|| panic!("{context} has no env table"));
    for (name, expected) in [
        ("CARGO_INCREMENTAL", "0"),
        ("RUSTC_WRAPPER", "sccache"),
        ("SCCACHE_GHA_ENABLED", "true"),
    ] {
        assert_eq!(
            env.get(name).and_then(Value::as_str),
            Some(expected),
            "{context} has the wrong {name}"
        );
    }
}

#[test]
fn sccache_env_is_bound_only_to_provisioned_apple_profiles() {
    let config: Value = toml::from_str(&workspace_file(".github-gen/velnor-workflow.toml"))
        .expect("parse Velnor workflow config");
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
        let env = profile.get("env").and_then(Value::as_table);
        if has_sccache {
            sccache_profile_ids.push(id);
            assert_sccache_env(profile, &format!("check profile {id}"));
        } else if let Some(env) = env {
            assert!(
                !env.contains_key("CARGO_INCREMENTAL"),
                "{id} enables incremental override without the Apple wrapper"
            );
            assert!(
                !env.contains_key("RUSTC_WRAPPER"),
                "{id} has an unprovisioned wrapper"
            );
            assert!(
                !env.contains_key("SCCACHE_GHA_ENABLED"),
                "{id} enables an unprovisioned sccache backend"
            );
        }
    }
    assert_eq!(
        sccache_profile_ids,
        ["desktop-merge", "desktop-scheduled"],
        "all cargo:sccache check profiles must be wired"
    );

    let units = config
        .get("units")
        .and_then(Value::as_array)
        .expect("units");
    let design_unit_id = "swift-package-native-design-prototypes-unifiedagentusage";
    let design_unit = units
        .iter()
        .find(|unit| unit.get("id").and_then(Value::as_str) == Some(design_unit_id))
        .unwrap_or_else(|| panic!("unit {design_unit_id}"));
    assert!(
        design_unit
            .get("mise_tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| {
                tools
                    .iter()
                    .any(|tool| tool.as_str() == Some("cargo:sccache"))
            }),
        "Swift member {design_unit_id} must provision the shared wrapper"
    );
    for id in [
        "swift-package-native",
        "swift-xcodegen-native-project-yml-jackindesktop",
    ] {
        let unit = units
            .iter()
            .find(|unit| unit.get("id").and_then(Value::as_str) == Some(id))
            .unwrap_or_else(|| panic!("unit {id}"));
        let env = unit.get("env").and_then(Value::as_table).expect("unit env");
        assert_eq!(env.len(), 3, "{id} has unsupported extra env fields");
        assert_sccache_env(unit, &format!("unit {id}"));
    }

    let swift = workspace_file(".github/workflows/ci-unit-swift.yml");
    for line in [
        "  CARGO_INCREMENTAL: \"0\"",
        "  RUSTC_WRAPPER: sccache",
        "  SCCACHE_GHA_ENABLED: \"true\"",
    ] {
        assert!(swift.contains(line), "Swift reusable job lacks `{line}`");
    }
    assert!(
        swift.find("- name: Set up Mise tools") < swift.find("- name: Run unit checks"),
        "Swift wrapper must be installed before checks"
    );

    for workflow in [
        ".github/workflows/ci-main.yml",
        ".github/workflows/ci-pr.yml",
    ] {
        let text = workspace_file(workflow);
        for id in [
            "swift-package-native-design-prototypes-unifiedagentusage",
            "swift-package-native",
            "swift-xcodegen-native-project-yml-jackindesktop",
        ] {
            let block = text
                .split("\n  github-hosted-")
                .find(|block| block.contains(&format!("unit: {id}")))
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
    }

    for workflow in [
        ".github/workflows/desktop-merge.yml",
        ".github/workflows/desktop-scheduled.yml",
    ] {
        let text = workspace_file(workflow);
        for line in [
            "CARGO_INCREMENTAL: \"0\"",
            "RUSTC_WRAPPER: sccache",
            "SCCACHE_GHA_ENABLED: \"true\"",
        ] {
            assert!(text.contains(line), "{workflow} lacks `{line}`");
        }
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
        for line in [
            "CARGO_INCREMENTAL: \"0\"",
            "RUSTC_WRAPPER: sccache",
            "SCCACHE_GHA_ENABLED: \"true\"",
        ] {
            assert!(!text.contains(line), "{workflow} has invalid `{line}`");
        }
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
