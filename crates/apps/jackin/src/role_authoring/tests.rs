// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `role_authoring`.
use super::*;
use tempfile::tempdir;

#[test]
fn scaffold_path_uses_jackin_prefix() {
    let selector = RoleSelector::parse("docs-writer").unwrap();
    assert_eq!(
        scaffold_path(Path::new("/projects"), &selector),
        Path::new("/projects/jackin-docs-writer")
    );
}

#[test]
fn scaffold_path_nests_namespaced_roles() {
    let selector = RoleSelector::parse("ChainArgos/Backend").unwrap();
    assert_eq!(
        scaffold_path(Path::new("/projects"), &selector),
        Path::new("/projects/chainargos/jackin-backend")
    );
}

#[test]
fn create_writes_a_valid_role_repo() {
    let temp = tempdir().unwrap();
    create(&RoleCreateArgs {
        role: "docs-writer".to_owned(),
        projects_dir: Some(temp.path().to_path_buf()),
    })
    .unwrap();

    let repo = temp.path().join("jackin-docs-writer");
    assert!(repo.join("jackin.role.toml").is_file());
    assert!(repo.join("Dockerfile").is_file());
    assert!(repo.join(".github/workflows/validate.yml").is_file());
    validate_role_repo(&repo).unwrap();
}

#[test]
fn display_name_title_cases_slug_words() {
    assert_eq!(display_name("backend-engineer"), "Backend Engineer");
}

#[test]
fn published_image_prints_declared_image() {
    let temp = tempdir().unwrap();
    write_scaffold_with_manifest(
        temp.path(),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role"

[claude]
plugins = []
"#,
    );
    published_image(RoleRepoPathArgs {
        path: Some(temp.path().to_path_buf()),
    })
    .unwrap();
}

#[test]
fn published_image_errors_when_not_declared() {
    let temp = tempdir().unwrap();
    write_scaffold_with_manifest(
        temp.path(),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    );
    let err = published_image(RoleRepoPathArgs {
        path: Some(temp.path().to_path_buf()),
    })
    .unwrap_err();
    assert!(format!("{err:#}").contains("no published_image"), "{err:#}");
}

fn write_scaffold_with_manifest(path: &Path, manifest: &str) {
    std::fs::write(
        path.join("Dockerfile"),
        "FROM projectjackin/construct:0.2-trixie\n",
    )
    .unwrap();
    std::fs::write(path.join("jackin.role.toml"), manifest).unwrap();
}
