// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn skips_prompt_when_role_supports_a_single_agent() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::parse("solo").unwrap();
    write_role_manifest(
        &jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir,
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#,
    );

    assert!(
        supported_agents_requiring_prompt(&paths, &selector, None).is_none(),
        "single-agent roles have nothing to disambiguate"
    );
}

#[test]
fn skips_prompt_when_manifest_is_missing_or_unreadable() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::parse("ghost").unwrap();
    // No manifest written — load_role will fetch and validate later.
    assert!(supported_agents_requiring_prompt(&paths, &selector, None).is_none());
}
