// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `repo`.
use super::*;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use tempfile::tempdir;

#[test]
fn computes_cached_repo_path_for_namespaced_selector() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");

    let repo = CachedRepo::new(&paths, &selector);

    assert_eq!(
        repo.repo_dir,
        paths
            .roles_dir
            .join("chainargos")
            .join("the-architect")
            .join("default")
    );
}

#[test]
fn computes_branch_cache_path_as_sibling_of_default() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "the-architect");

    let repo = CachedRepo::for_branch(&paths, &selector, "feat/caveman-all-install");

    assert_eq!(
        repo.repo_dir,
        paths
            .roles_dir
            .join("the-architect")
            .join("branches")
            .join("feat")
            .join("caveman-all-install")
    );
}

#[test]
fn rejects_repo_without_required_files() {
    let temp = tempdir().unwrap();
    std::fs::create_dir_all(temp.path()).unwrap();

    let error = validate_role_repo(temp.path()).unwrap_err();

    assert!(error.to_string().contains("jackin.role.toml"));
}

#[test]
fn rejects_missing_manifest_dockerfile() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "docker/role.Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let error = validate_role_repo(temp.path()).unwrap_err();

    assert!(error.to_string().contains("docker/role.Dockerfile"));
}

#[test]
fn rejects_manifest_dockerfile_outside_repo() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "../Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let error = validate_role_repo(temp.path()).unwrap_err();

    assert!(error.to_string().contains("must stay inside the repo"));
}

#[cfg(unix)]
#[test]
fn rejects_symlink_escaping_repo_boundary() {
    let temp = tempdir().unwrap();
    let outside = tempdir().unwrap();
    std::fs::write(outside.path().join("Dockerfile"), "FROM debian:trixie\n").unwrap();
    std::os::unix::fs::symlink(outside.path(), temp.path().join("escape")).unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "escape/Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let error = validate_role_repo(temp.path()).unwrap_err();

    assert!(error.to_string().contains("escapes the repo boundary"));
}

#[test]
fn accepts_manifest_dockerfile_in_subdirectory() {
    let temp = tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("docker")).unwrap();
    std::fs::write(
        temp.path().join("docker/role.Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "docker/role.Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let validated = validate_role_repo(temp.path()).unwrap();

    assert_eq!(
        validated.dockerfile.dockerfile_path,
        temp.path()
            .canonicalize()
            .unwrap()
            .join("docker/role.Dockerfile")
    );
}

#[test]
fn accepts_manifest_with_valid_preflight_hook() {
    let temp = tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("hooks")).unwrap();
    std::fs::write(
        temp.path().join("hooks/preflight.sh"),
        r"#!/bin/bash
echo hello
",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
preflight = "hooks/preflight.sh"
"#,
    )
    .unwrap();

    let validated = validate_role_repo(temp.path()).unwrap();

    assert!(
        validated
            .manifest
            .hooks
            .as_ref()
            .unwrap()
            .preflight
            .is_some()
    );
}

#[test]
fn accepts_manifest_with_runtime_hooks() {
    let temp = tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("hooks")).unwrap();
    for name in ["setup-once.sh", "source.sh", "preflight.sh"] {
        std::fs::write(
            temp.path().join("hooks").join(name),
            "#!/bin/bash\necho ok\n",
        )
        .unwrap();
    }
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
setup_once = "hooks/setup-once.sh"
source = "hooks/source.sh"
preflight = "hooks/preflight.sh"
"#,
    )
    .unwrap();

    let validated = validate_role_repo(temp.path()).unwrap();
    let hooks = validated.manifest.hooks.as_ref().unwrap();

    assert!(hooks.setup_once.is_some());
    assert!(hooks.source.is_some());
    assert!(hooks.preflight.is_some());
}

#[test]
fn rejects_preflight_hook_outside_repo() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
preflight = "../escape.sh"
"#,
    )
    .unwrap();

    let error = validate_role_repo(temp.path()).unwrap_err();

    assert!(error.to_string().contains("must stay inside the repo"));
}

#[test]
fn rejects_preflight_hook_that_does_not_exist() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
preflight = "hooks/missing.sh"
"#,
    )
    .unwrap();

    let error = validate_role_repo(temp.path()).unwrap_err();

    assert!(error.to_string().contains("missing"));
}

#[test]
fn rejects_absolute_preflight_hook_path() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
preflight = "/etc/evil.sh"
"#,
    )
    .unwrap();

    let error = validate_role_repo(temp.path()).unwrap_err();

    assert!(error.to_string().contains("must be relative"));
}

#[test]
fn rejects_empty_preflight_hook() {
    let error = empty_hook_error("preflight", "hooks/preflight.sh");
    assert!(error.contains("preflight hook is empty"));
}

#[test]
fn rejects_empty_setup_once_hook() {
    let error = empty_hook_error("setup_once", "hooks/setup-once.sh");
    assert!(error.contains("setup_once hook is empty"));
}

#[test]
fn rejects_empty_source_hook() {
    let error = empty_hook_error("source", "hooks/source.sh");
    assert!(error.contains("source hook is empty"));
}

fn empty_hook_error(field: &str, path: &str) -> String {
    let temp = tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("hooks")).unwrap();
    std::fs::write(temp.path().join(path), "").unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        format!(
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
{field} = "{path}"
"#
        ),
    )
    .unwrap();

    validate_role_repo(temp.path()).unwrap_err().to_string()
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_preflight_hook_inside_repo() {
    let temp = tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("hooks")).unwrap();
    std::fs::write(temp.path().join("real-hook.sh"), "#!/bin/bash\necho hi\n").unwrap();
    std::os::unix::fs::symlink(
        temp.path().join("real-hook.sh"),
        temp.path().join("hooks/preflight.sh"),
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
preflight = "hooks/preflight.sh"
"#,
    )
    .unwrap();

    let error = validate_role_repo(temp.path()).unwrap_err();

    assert!(error.to_string().contains("symlink"));
    assert!(error.to_string().contains("preflight"));
}
