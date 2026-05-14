use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

fn write_role_repo(path: &std::path::Path, manifest: &str) {
    std::fs::write(
        path.join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
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
            "Migrated manifest v1alpha2 -> v1alpha3",
        ))
        .stdout(predicate::str::contains("Role repository is valid"));

    let manifest = std::fs::read_to_string(temp.path().join("jackin.role.toml")).unwrap();
    assert!(manifest.starts_with("version = \"v1alpha3\""), "{manifest}");
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
