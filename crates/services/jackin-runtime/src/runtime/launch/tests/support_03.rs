// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn programmatic_role_fixture(
    paths: &JackinPaths,
    selector: &RoleSelector,
) -> jackin_config::ResolvedWorkspace {
    let repo_dir = jackin_manifest::repo::CachedRepo::new(paths, selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        "version = \"v1alpha3\"\ndockerfile = \"Dockerfile\"\n",
    )
    .unwrap();
    repo_workspace(&repo_dir)
}
