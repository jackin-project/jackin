// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn seed_valid_role_repo(repo_dir: &std::path::Path) {
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        jackin_manifest::repo_contract::BASE_DOCKERFILE_FROM,
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
}

pub(super) fn validated_test_repo(
    paths: &JackinPaths,
    selector: &RoleSelector,
) -> (CachedRepo, jackin_manifest::repo::ValidatedRoleRepo) {
    let cached_repo = CachedRepo::new(paths, selector);
    seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    (cached_repo, validated_repo)
}
