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
fn sccache_env_is_bound_only_to_provisioned_cargo_profiles() {
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

    let rust = workspace_file(".github/workflows/ci-unit-rust.yml");
    assert!(!rust.contains("RUSTC_WRAPPER: sccache"));
    assert!(!rust.contains("SCCACHE_GHA_ENABLED: \"true\""));

    let release = workspace_file(".github/workflows/release.yml");
    assert!(!release.contains("RUSTC_WRAPPER: sccache"));
    assert!(!release.contains("SCCACHE_GHA_ENABLED: \"true\""));
}
