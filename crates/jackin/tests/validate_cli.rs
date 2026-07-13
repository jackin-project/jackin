#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::manual_assert,
    clippy::duration_suboptimal_units,
    clippy::filter_map_next,
    clippy::map_unwrap_or,
    clippy::redundant_closure,
    unreachable_pub,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn write_role_repo(temp: &tempfile::TempDir, dockerfile: &str, manifest: &str) {
    std::fs::write(temp.path().join("Dockerfile"), dockerfile).unwrap();
    std::fs::write(temp.path().join("jackin.role.toml"), manifest).unwrap();
}

const VALID_MANIFEST: &str = r#"version = "v1alpha6"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#;

const PUBLISHED_MANIFEST: &str = r#"version = "v1alpha5"
dockerfile = "Dockerfile"
published_image = "ghcr.io/example/jackin-sentinel:latest"

[claude]
plugins = []
"#;

const VERSIONED_FROM: &str = "FROM projectjackin/construct:0.1-trixie\n";

// ── validate subcommand ──────────────────────────────────────────────────────

#[test]
fn validate_passes_for_valid_agent_repo() {
    let temp = tempdir().unwrap();
    write_role_repo(&temp, VERSIONED_FROM, VALID_MANIFEST);
    std::fs::write(temp.path().join(".dockerignore"), ".git\n").unwrap();
    std::fs::write(temp.path().join(".gitignore"), "target/\n").unwrap();

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["validate", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Role repository is valid"));
}

#[test]
fn validate_passes_for_jackin_sentinel_fixture_role() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/roles/jackin-sentinel");

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["validate", fixture.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Role repository is valid"));
}

#[test]
fn validate_fails_for_wrong_base_image() {
    let temp = tempdir().unwrap();
    write_role_repo(
        &temp,
        "FROM debian:trixie\nRUN echo hello\n",
        VALID_MANIFEST,
    );
    std::fs::write(temp.path().join(".dockerignore"), ".git\n").unwrap();
    std::fs::write(temp.path().join(".gitignore"), "target/\n").unwrap();

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["validate", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("projectjackin/construct:trixie"));
}

#[test]
fn validate_rejects_floating_construct_tag() {
    let temp = tempdir().unwrap();
    write_role_repo(
        &temp,
        "FROM projectjackin/construct:trixie\n",
        VALID_MANIFEST,
    );

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["validate", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("floating tag"));
}

#[test]
fn validate_allows_missing_dockerignore() {
    let temp = tempdir().unwrap();
    write_role_repo(&temp, VERSIONED_FROM, VALID_MANIFEST);
    std::fs::write(temp.path().join(".gitignore"), "target/\n").unwrap();

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["validate", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Role repository is valid"));
}

#[test]
fn validate_fails_for_invalid_manifest() {
    let temp = tempdir().unwrap();
    std::fs::write(temp.path().join("Dockerfile"), VERSIONED_FROM).unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha6"
dockerfile = "Dockerfile"
unknown_field = true

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(temp.path().join(".dockerignore"), ".git\n").unwrap();
    std::fs::write(temp.path().join(".gitignore"), "target/\n").unwrap();

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["validate", temp.path().to_str().unwrap()])
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
        "FROM projectjackin/construct:0.1-trixie\nRUN echo hello\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha6"
dockerfile = "docker/role.Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["validate", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Role repository is valid"));
}

#[test]
fn validate_fails_for_invalid_preflight_hook() {
    let temp = tempdir().unwrap();
    write_role_repo(
        &temp,
        VERSIONED_FROM,
        r#"version = "v1alpha6"
dockerfile = "Dockerfile"

[hooks]
preflight = "hooks/preflight.sh"

[claude]
plugins = []
"#,
    );

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["validate", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("hooks/preflight.sh"));
}

#[test]
fn validate_with_no_args_shows_usage() {
    Command::cargo_bin("jackin-role")
        .unwrap()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

// ── migrate subcommand ───────────────────────────────────────────────────────

#[test]
fn migrate_is_noop_for_current_manifest() {
    let temp = tempdir().unwrap();
    write_role_repo(&temp, VERSIONED_FROM, VALID_MANIFEST);

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["migrate", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Manifest already at current version",
        ));
}

#[test]
fn migrate_updates_legacy_manifest() {
    let temp = tempdir().unwrap();
    write_role_repo(
        &temp,
        VERSIONED_FROM,
        "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
    );

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["migrate", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Migrated manifest legacy -> v1alpha6",
        ))
        .stdout(predicate::str::contains("Role repository is valid"));

    let out = std::fs::read_to_string(temp.path().join("jackin.role.toml")).unwrap();
    assert!(out.contains(r#"version = "v1alpha6""#), "{out}");
}

#[test]
fn migrate_rejects_newer_manifest_version() {
    let temp = tempdir().unwrap();
    write_role_repo(
        &temp,
        VERSIONED_FROM,
        "version = \"v2alpha1\"\ndockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
    );

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["migrate", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("only understands up to v1alpha6"));
}

#[test]
fn migrate_reports_missing_manifest() {
    let temp = tempdir().unwrap();

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["migrate", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("jackin.role.toml"))
        .stderr(predicate::str::contains("reading"));
}

#[test]
fn migrate_reports_malformed_manifest() {
    let temp = tempdir().unwrap();
    std::fs::write(temp.path().join("jackin.role.toml"), "this = is = invalid").unwrap();

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["migrate", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("jackin.role.toml"))
        .stderr(predicate::str::contains("parsing"));
}

// ── construct-version subcommand ─────────────────────────────────────────────

#[test]
fn construct_version_prints_tag() {
    let temp = tempdir().unwrap();
    write_role_repo(&temp, VERSIONED_FROM, VALID_MANIFEST);

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["construct-version", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("0.1-trixie"));
}

#[test]
fn construct_version_strips_digest_pin() {
    let temp = tempdir().unwrap();
    write_role_repo(
        &temp,
        "FROM projectjackin/construct:0.2-trixie@sha256:0b076bfbc53d36794fe54b1a9cab670f85f831af86d78426b1a88a8ac192d445\n",
        VALID_MANIFEST,
    );

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["construct-version", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout("0.2-trixie\n");
}

#[test]
fn construct_version_fails_for_invalid_repo() {
    let temp = tempdir().unwrap();
    write_role_repo(
        &temp,
        "FROM projectjackin/construct:trixie\n",
        VALID_MANIFEST,
    );

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["construct-version", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("floating tag"));
}

// ── published-image-repository subcommand ───────────────────────────────────

#[test]
fn published_image_repository_strips_tag() {
    let temp = tempdir().unwrap();
    write_role_repo(&temp, VERSIONED_FROM, PUBLISHED_MANIFEST);

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args(["published-image-repository", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout("ghcr.io/example/jackin-sentinel\n");
}

// ── publish-labels subcommand ───────────────────────────────────────────────

#[test]
fn publish_labels_prints_canonical_contract() {
    let temp = tempdir().unwrap();
    write_role_repo(&temp, VERSIONED_FROM, PUBLISHED_MANIFEST);

    Command::cargo_bin("jackin-role")
        .unwrap()
        .args([
            "publish-labels",
            "--role-git-sha",
            "abcdef123456",
            temp.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout("jackin.construct.version=0.1-trixie\njackin.role.git.sha=abcdef123456\n");
}
