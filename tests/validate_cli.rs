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
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
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
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
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
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
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
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\nunknown_field = true\n\n[claude]\nplugins = []\n",
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
        temp.path().join("docker/agent.Dockerfile"),
        "FROM projectjackin/construct:trixie\nRUN echo hello\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"docker/agent.Dockerfile\"\n\n[claude]\nplugins = []\n",
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
        temp.path().join("jackin.agent.toml"),
        "dockerfile = \"Dockerfile\"\n\n[hooks]\npre_launch = \"hooks/pre-launch.sh\"\n\n[claude]\nplugins = []\n",
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
