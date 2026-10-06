// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn remote_points_at_github_covers_all_three_forms() {
    assert!(remote_points_at_github("git@github.com:o/r.git"));
    assert!(remote_points_at_github("https://github.com/o/r.git"));
    assert!(remote_points_at_github("http://github.com/o/r"));
    assert!(remote_points_at_github("ssh://git@github.com/o/r.git"));
    assert!(remote_points_at_github("ssh://github.com/o/r.git"));
    // Non-GitHub hosts reject.
    assert!(!remote_points_at_github("git@gitlab.com:o/r.git"));
    assert!(!remote_points_at_github("https://gitlab.com/o/r.git"));
    assert!(!remote_points_at_github("git@git.example.com:o/r.git"));
    // GitHub-lookalike subdomain does not count.
    assert!(!remote_points_at_github(
        "https://github.com.evil.example/o/r"
    ));
}

#[test]
fn parse_origin_url_from_config() {
    let config = r#"
[core]
    repositoryformatversion = 0

[remote "origin"]
    url = git@github.com:owner/repo.git
    fetch = +refs/heads/*:refs/remotes/origin/*

[branch "main"]
    remote = origin
"#;
    assert_eq!(
        parse_remote_origin_url(config),
        Some("git@github.com:owner/repo.git".to_owned())
    );
}
