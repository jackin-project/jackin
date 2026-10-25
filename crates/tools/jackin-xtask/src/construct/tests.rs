use super::*;

#[test]
fn inspect_success_means_already_published() {
    assert_eq!(classify_inspect(true, ""), VersionStatus::AlreadyPublished);
}

#[test]
fn inspect_known_absent_markers_mean_unpublished() {
    for stderr in [
        "ERROR: manifest unknown: manifest unknown not found",
        "MANIFEST_UNKNOWN: manifest unknown",
        "requested tag does not exist",
        "NAME_UNKNOWN: repository name not known",
    ] {
        assert_eq!(
            classify_inspect(false, stderr),
            VersionStatus::Unpublished,
            "{stderr}"
        );
    }
}

#[test]
fn inspect_other_failure_is_unknown_error() {
    assert_eq!(
        classify_inspect(false, "unauthorized: authentication required"),
        VersionStatus::UnknownError
    );
}

#[test]
fn published_version_skips_manifest_publish() {
    assert_eq!(manifest_action(true), ManifestAction::SkipAlreadyPublished);
}

#[test]
fn unpublished_version_publishes_manifest() {
    assert_eq!(manifest_action(false), ManifestAction::Publish);
}

#[test]
fn find_digest_reads_nested_target() {
    let value = serde_json::json!({
        "construct-publish": { "containerimage.digest": "sha256:abc123" }
    });
    assert_eq!(find_digest(&value).as_deref(), Some("sha256:abc123"));
}

#[test]
fn find_digest_ignores_empty_and_missing() {
    assert_eq!(
        find_digest(&serde_json::json!({"t": {"containerimage.digest": ""}})),
        None
    );
    assert_eq!(find_digest(&serde_json::json!({"t": {"other": "x"}})), None);
}

#[test]
fn construct_build_argument_uses_the_root_mise_minimum() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    if std::env::var_os("JACKIN_TEST_CONSTRUCT_ROOT").is_none() {
        for override_version in ["", "1900.1.1"] {
            let mut child = Command::new(std::env::current_exe().unwrap());
            child
                .args([
                    "--exact",
                    "construct::tests::construct_build_argument_uses_the_root_mise_minimum",
                    "--nocapture",
                ])
                .current_dir(&root)
                .env("JACKIN_TEST_CONSTRUCT_ROOT", "1");
            if !override_version.is_empty() {
                child.env("MISE_VERSION", override_version);
            }
            crate::cmd::run(&mut child).unwrap();
        }
        return;
    }
    let source = fs::read_to_string(root.join("mise.toml")).unwrap();
    let config: toml::Value = toml::from_str(&source).unwrap();
    let minimum = config["min_version"].as_str().unwrap();
    let resolved = Config::resolve().unwrap();
    assert_eq!(resolved.mise_version, minimum);
    let mut command = Command::new("docker");
    resolved.apply_bake_env(&mut command);
    let selected = command
        .get_envs()
        .find(|(name, _)| *name == "MISE_VERSION")
        .and_then(|(_, value)| value)
        .unwrap();
    assert_eq!(selected, minimum);
}

#[test]
fn construct_mise_minimum_is_required_and_exact() {
    assert_eq!(
        versions::parse_mise_minimum("min_version = \"2026.10.17\"").unwrap(),
        "2026.10.17"
    );
    for invalid in [
        "",
        "min_version = 17",
        "min_version = \"\"",
        "min_version = \"latest\"",
        "min_version = \"2026.10\"",
        "min_version = \"2026.10.17/other\"",
    ] {
        assert!(versions::parse_mise_minimum(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn construct_has_no_duplicate_mise_pin_or_docker_default() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let versions = fs::read_to_string(root.join("docker/construct/versions.env")).unwrap();
    assert!(
        !versions
            .lines()
            .any(|line| line.starts_with("MISE_VERSION="))
    );
    let dockerfile = fs::read_to_string(root.join("docker/construct/Dockerfile")).unwrap();
    assert!(
        !dockerfile
            .lines()
            .any(|line| line.starts_with("ARG MISE_VERSION="))
    );
    assert!(dockerfile.lines().any(|line| line == "ARG MISE_VERSION"));
    assert!(dockerfile.contains("MISE_VERSION build arg is required"));
}

#[test]
fn upstream_asset_check_uses_the_same_root_mise_minimum_without_network() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let fixture = tempfile::tempdir().unwrap();
    let construct = fixture.path().join("docker/construct");
    fs::create_dir_all(&construct).unwrap();
    for name in ["check-upstream-assets.sh", "versions.env"] {
        fs::copy(
            root.join("docker/construct").join(name),
            construct.join(name),
        )
        .unwrap();
    }
    fs::write(
        fixture.path().join("mise.toml"),
        "min_version = \"2027.1.2\"\n",
    )
    .unwrap();
    let bin = fixture.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let curl = bin.join("curl");
    fs::write(
        &curl,
        r#"#!/bin/sh
case "$*" in
  *github.com/jdx/mise/*) printf '%s\n' "$*" > "$JACKIN_TEST_URL"; exit 76 ;;
esac
exit 0
"#,
    )
    .unwrap();
    fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
    let mise = bin.join("mise");
    fs::write(
        &mise,
        r#"#!/bin/sh
printf '%s\n' "$@" > "$JACKIN_TEST_MISE_ARGS"
printf '%s\n' "${JACKIN_TEST_MISE_VERSION:-2027.1.2}"
"#,
    )
    .unwrap();
    fs::set_permissions(&mise, fs::Permissions::from_mode(0o755)).unwrap();
    let captured = fixture.path().join("mise-url");
    let mise_args = fixture.path().join("mise-args");
    let mut command = Command::new("bash");
    command
        .arg(construct.join("check-upstream-assets.sh"))
        .env("JACKIN_TEST_URL", &captured)
        .env("JACKIN_TEST_MISE_ARGS", &mise_args)
        .env("MISE_VERSION", "1900.1.1")
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        );
    crate::cmd::output_timeout(&mut command, std::time::Duration::from_secs(10)).unwrap_err();
    let args = fs::read_to_string(&mise_args).unwrap();
    let args: Vec<_> = args.lines().collect();
    assert_eq!(
        &args[..6],
        [
            "--no-config",
            "--no-env",
            "--no-hooks",
            "config",
            "get",
            "--file"
        ]
    );
    assert!(args[6].ends_with("/mise.toml"));
    assert_eq!(args[7], "min_version");
    let url = fs::read_to_string(captured).unwrap();
    assert!(url.contains("https://github.com/jdx/mise/releases/download/v2027.1.2/SHASUMS256.txt"));
    assert!(!url.contains("1900.1.1"));

    let invalid_url = fixture.path().join("invalid-mise-url");
    let mut invalid = Command::new("bash");
    invalid
        .arg(construct.join("check-upstream-assets.sh"))
        .env("JACKIN_TEST_URL", &invalid_url)
        .env("JACKIN_TEST_MISE_ARGS", &mise_args)
        .env("JACKIN_TEST_MISE_VERSION", "2027.1.2/other")
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        );
    crate::cmd::output_timeout(&mut invalid, std::time::Duration::from_secs(10)).unwrap_err();
    assert!(!invalid_url.exists());
}
