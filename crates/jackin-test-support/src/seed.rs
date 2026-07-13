//! Minimal valid role-repo fixtures for tests exercising `validate_role_repo`
//! and role-registration temp-dir discovery.

#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test support fixture setup should fail immediately with source location"
)]

/// Minimal Dockerfile content used in test role repos. Passes `validate_agent_dockerfile`.
pub const TEST_DOCKERFILE_FROM: &str = jackin_manifest::repo_contract::BASE_DOCKERFILE_FROM;

/// Minimal `jackin.role.toml` content used in test role repos. Parses as a valid manifest.
const TEST_MANIFEST_TOML: &str = r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#;

/// Seed a minimal but valid role repo at `repo_dir`.
///
/// Creates `.git/`, `Dockerfile`, and `jackin.role.toml`. All three are
/// required for `validate_role_repo` to succeed.
pub fn seed_valid_role_repo(repo_dir: &std::path::Path) {
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(repo_dir.join("Dockerfile"), TEST_DOCKERFILE_FROM).unwrap();
    std::fs::write(repo_dir.join("jackin.role.toml"), TEST_MANIFEST_TOML).unwrap();
}

/// Find the `repo` subdir under the first `role-resolve-*` temp dir that
/// `register_agent_repo` creates inside `data_dir`.
pub fn first_temp_role_repo(data_dir: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_dir(data_dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("role-resolve-"))
        })
        .expect("role registration temp dir should exist before git clone side-effect")
        .join("repo")
}
