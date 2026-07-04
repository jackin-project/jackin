#![expect(

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

    clippy::unwrap_used,
    reason = "integration test fixture setup should fail immediately with source location"
)]

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

fn write_role_repo(path: &std::path::Path, manifest: &str) {
    std::fs::write(
        path.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(path.join("jackin.role.toml"), manifest).unwrap();
}

#[test]
fn role_validate_accepts_valid_repo() {
    let temp = tempfile::tempdir().unwrap();
    write_role_repo(
        temp.path(),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    );

    Command::cargo_bin("jackin")
        .unwrap()
        .args(["role", "validate", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Role repository is valid"));
}

#[test]
fn role_migrate_updates_then_validates_manifest() {
    let temp = tempfile::tempdir().unwrap();
    write_role_repo(
        temp.path(),
        r#"version = "v1alpha2"
dockerfile = "Dockerfile"
agents = ["opencode"]

[opencode]
"#,
    );

    Command::cargo_bin("jackin")
        .unwrap()
        .args(["role", "migrate", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Migrated manifest v1alpha2 -> v1alpha6",
        ))
        .stdout(predicate::str::contains("Role repository is valid"));

    let manifest = std::fs::read_to_string(temp.path().join("jackin.role.toml")).unwrap();
    assert!(manifest.starts_with("version = \"v1alpha6\""), "{manifest}");
}

#[test]
fn role_create_scaffolds_valid_repo_without_touching_config() {
    let temp = tempfile::tempdir().unwrap();
    let config_dir = temp.path().join("config");
    let projects_dir = temp.path().join("projects");

    Command::cargo_bin("jackin")
        .unwrap()
        .env("JACKIN_CONFIG_DIR", &config_dir)
        .args([
            "role",
            "create",
            "docs-writer",
            projects_dir.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created role repository"));

    let repo = projects_dir.join("jackin-docs-writer");
    assert!(repo.join("jackin.role.toml").is_file());
    assert!(repo.join("Dockerfile").is_file());
    assert!(repo.join(".github/workflows/validate.yml").is_file());
    assert!(!config_dir.exists());

    Command::cargo_bin("jackin")
        .unwrap()
        .args(["role", "validate", repo.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn role_create_normalizes_namespaced_role_to_lowercase() {
    let temp = tempfile::tempdir().unwrap();
    let projects_dir = temp.path().join("projects");

    Command::cargo_bin("jackin")
        .unwrap()
        .args([
            "role",
            "create",
            "ChainArgos/Backend",
            projects_dir.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("chainargos/jackin-backend"));

    let repo = projects_dir.join("chainargos/jackin-backend");
    assert!(repo.join("jackin.role.toml").is_file());
    let project_entries: Vec<String> = std::fs::read_dir(&projects_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(project_entries, vec!["chainargos"]);
    let namespace_entries: Vec<String> = std::fs::read_dir(projects_dir.join("chainargos"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(namespace_entries, vec!["jackin-backend"]);

    let readme = std::fs::read_to_string(repo.join("README.md")).unwrap();
    assert!(readme.contains("`chainargos/backend`"), "{readme}");
}

#[test]
fn role_published_image_prints_declared_image() {
    let temp = tempfile::tempdir().unwrap();
    write_role_repo(
        temp.path(),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role"

[claude]
plugins = []
"#,
    );

    Command::cargo_bin("jackin")
        .unwrap()
        .args(["role", "published-image", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout("docker.io/myorg/my-role\n");
}

#[test]
fn role_published_image_errors_when_not_declared() {
    let temp = tempfile::tempdir().unwrap();
    write_role_repo(
        temp.path(),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    );

    Command::cargo_bin("jackin")
        .unwrap()
        .args(["role", "published-image", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no published_image"));
}
