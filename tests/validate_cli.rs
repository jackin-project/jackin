use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn validate_passes_for_valid_agent_repo() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\nRUN echo hello\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha1"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(temp.path().join(".dockerignore"), ".git\n").unwrap();
    std::fs::write(temp.path().join(".gitignore"), "target/\n").unwrap();

    Command::cargo_bin("jackin-validate")
        .unwrap()
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("All checks passed"));
}

#[test]
fn validate_fails_for_wrong_base_image() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM debian:trixie\nRUN echo hello\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha1"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(temp.path().join(".dockerignore"), ".git\n").unwrap();
    std::fs::write(temp.path().join(".gitignore"), "target/\n").unwrap();

    Command::cargo_bin("jackin-validate")
        .unwrap()
        .arg(temp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("projectjackin/construct:trixie"));
}

#[test]
fn validate_allows_missing_dockerignore() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha1"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(temp.path().join(".gitignore"), "target/\n").unwrap();

    Command::cargo_bin("jackin-validate")
        .unwrap()
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("All checks passed"));
}

#[test]
fn validate_fails_for_invalid_manifest() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha1"
dockerfile = "Dockerfile"
unknown_field = true

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(temp.path().join(".dockerignore"), ".git\n").unwrap();
    std::fs::write(temp.path().join(".gitignore"), "target/\n").unwrap();

    Command::cargo_bin("jackin-validate")
        .unwrap()
        .arg(temp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown field"));
}

#[test]
fn validate_passes_when_manifest_uses_dockerfile_in_subdirectory() {
    let temp = tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("docker")).unwrap();
    std::fs::write(
        temp.path().join("docker/role.Dockerfile"),
        "FROM projectjackin/construct:trixie\nRUN echo hello\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha1"
dockerfile = "docker/role.Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    Command::cargo_bin("jackin-validate")
        .unwrap()
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("All checks passed"));
}

#[test]
fn validate_fails_for_invalid_pre_launch_hook() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha1"
dockerfile = "Dockerfile"

[hooks]
pre_launch = "hooks/pre-launch.sh"

[claude]
plugins = []
"#,
    )
    .unwrap();

    Command::cargo_bin("jackin-validate")
        .unwrap()
        .arg(temp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("hooks/pre-launch.sh"));
}

#[test]
fn validate_fails_with_no_args() {
    Command::cargo_bin("jackin-validate")
        .unwrap()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn validate_migrate_accepts_flag_after_path() {
    // `--migrate` may appear in any position relative to <role-repo-path>.
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
    )
    .unwrap();

    Command::cargo_bin("jackin-validate")
        .unwrap()
        .args([temp.path().to_str().unwrap(), "--migrate"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Migrated manifest legacy -> v1alpha1",
        ));
}

#[test]
fn validate_migrate_is_noop_for_current_manifest() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        "version = \"v1alpha1\"\ndockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
    )
    .unwrap();

    Command::cargo_bin("jackin-validate")
        .unwrap()
        .args(["--migrate", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Manifest already at current version",
        ));
}

#[test]
fn validate_migrate_rejects_newer_manifest_version() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        "version = \"v2alpha1\"\ndockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
    )
    .unwrap();

    Command::cargo_bin("jackin-validate")
        .unwrap()
        .args(["--migrate", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("only understands up to v1alpha1"));
}

#[test]
fn validate_migrate_reports_missing_manifest() {
    let temp = tempdir().unwrap();

    Command::cargo_bin("jackin-validate")
        .unwrap()
        .args(["--migrate", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("reading"));
}

#[test]
fn validate_migrate_reports_malformed_manifest() {
    let temp = tempdir().unwrap();
    std::fs::write(temp.path().join("jackin.role.toml"), "this = is = invalid").unwrap();

    Command::cargo_bin("jackin-validate")
        .unwrap()
        .args(["--migrate", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("parsing"));
}

#[test]
fn validate_rejects_unknown_flag() {
    Command::cargo_bin("jackin-validate")
        .unwrap()
        .args(["--unknown", "/tmp"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown flag"));
}

#[test]
fn validate_rejects_too_many_positional_args() {
    Command::cargo_bin("jackin-validate")
        .unwrap()
        .args(["/tmp/a", "/tmp/b"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("too many positional arguments"));
}

#[test]
fn validate_migrate_updates_legacy_manifest() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
    )
    .unwrap();

    Command::cargo_bin("jackin-validate")
        .unwrap()
        .args(["--migrate", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Migrated manifest legacy -> v1alpha1",
        ))
        .stdout(predicate::str::contains("All checks passed"));

    let out = std::fs::read_to_string(temp.path().join("jackin.role.toml")).unwrap();
    assert!(out.contains(r#"version = "v1alpha1""#), "{out}");
}
