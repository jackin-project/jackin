// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn normalize_github_url_rewrites_scp_form() {
    assert_eq!(
        normalize_github_url("git@github.com:jackin-project/jackin.git"),
        "https://github.com/jackin-project/jackin.git"
    );
}

#[test]
fn normalize_github_url_rewrites_ssh_url_form() {
    assert_eq!(
        normalize_github_url("ssh://git@github.com/jackin-project/jackin.git"),
        "https://github.com/jackin-project/jackin.git"
    );
}

#[test]
fn normalize_github_url_passes_https_through_unchanged() {
    assert_eq!(
        normalize_github_url("https://github.com/jackin-project/jackin.git"),
        "https://github.com/jackin-project/jackin.git"
    );
}

#[test]
fn normalize_github_url_leaves_non_github_urls_alone() {
    // Non-GitHub SSH URLs must NOT be rewritten — substituting an
    // HTTPS URL would risk hitting an endpoint that doesn't exist
    // on the operator's SCM.
    assert_eq!(
        normalize_github_url("git@gitlab.example.com:team/repo.git"),
        "git@gitlab.example.com:team/repo.git"
    );
    assert_eq!(
        normalize_github_url("ssh://git@gitlab.example.com/team/repo.git"),
        "ssh://git@gitlab.example.com/team/repo.git"
    );
}

#[test]
fn normalize_github_url_handles_missing_git_suffix() {
    assert_eq!(
        normalize_github_url("git@github.com:jackin-project/jackin"),
        "https://github.com/jackin-project/jackin"
    );
    assert_eq!(
        normalize_github_url("ssh://git@github.com/jackin-project/jackin"),
        "https://github.com/jackin-project/jackin"
    );
}

#[test]
fn repo_matches_cross_protocol_for_same_owner_repo() {
    // After SSH→HTTPS normalize, a repo cloned years ago via SSH
    // and a config that now says HTTPS must agree at the
    // remote-URL match check.
    assert!(repo_matches(
        "https://github.com/jackin-project/jackin.git",
        "git@github.com:jackin-project/jackin.git"
    ));
    assert!(repo_matches(
        "git@github.com:jackin-project/jackin.git",
        "https://github.com/jackin-project/jackin.git"
    ));
}

#[test]
fn parse_repo_name_extracts_owner_repo_from_ssh_url() {
    assert_eq!(
        parse_repo_name("git@github.com:jackin-project/jackin.git"),
        Some("jackin-project/jackin".to_owned())
    );
}

#[test]
fn parse_repo_name_extracts_owner_repo_from_https_url() {
    assert_eq!(
        parse_repo_name("https://github.com/jackin-project/jackin.git"),
        Some("jackin-project/jackin".to_owned())
    );
}

#[test]
fn parse_repo_name_handles_url_without_git_suffix() {
    assert_eq!(
        parse_repo_name("https://github.com/jackin-project/jackin"),
        Some("jackin-project/jackin".to_owned())
    );
    assert_eq!(
        parse_repo_name("git@github.com:jackin-project/jackin"),
        Some("jackin-project/jackin".to_owned())
    );
}

#[tokio::test]
async fn resolve_agent_repo_rejects_cached_repo_with_wrong_remote() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let mut runner =
        FakeRunner::with_capture_queue(["git@github.com:evil/agent-smith.git".to_owned()]);
    let error = resolve_agent_repo(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        false,
        None,
    )
    .await
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("cached role repo remote mismatch")
    );
}

#[tokio::test]
async fn resolve_agent_repo_recovers_when_user_confirms_removal() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    // The capture queue provides: 1) the wrong remote URL, then 2) a
    // successful clone response (empty output).  After the user confirms,
    // the function removes the stale dir and re-clones.
    let mut runner = FakeRunner::with_capture_queue([
        "git@github.com:evil/agent-smith.git".to_owned(),
        String::new(), // clone output
    ]);

    // Simulate what `git clone` would produce on disk: recreate the repo
    // files when the clone command is captured by FakeRunner.
    let repo_dir_clone = repo_dir;
    runner.side_effects.push((
        "clone".to_owned(),
        Box::new(move || {
            std::fs::create_dir_all(repo_dir_clone.join(".git")).unwrap();
            std::fs::write(
                repo_dir_clone.join("Dockerfile"),
                "FROM projectjackin/construct:0.1-trixie\n",
            )
            .unwrap();
            std::fs::write(
                repo_dir_clone.join("jackin.role.toml"),
                r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
            )
            .unwrap();
        }),
    ));

    let result = resolve_agent_repo_with(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        RepoResolveOptions::interactive(false),
        || Ok(true), // user confirms removal
    )
    .await;

    result.expect("expected recovery to succeed");
    assert!(
        runner.recorded.iter().any(|c| c.contains("clone")),
        "expected a git clone after removal"
    );
}

#[tokio::test]
async fn resolve_agent_repo_aborts_when_user_declines_removal() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let mut runner =
        FakeRunner::with_capture_queue(["git@github.com:evil/agent-smith.git".to_owned()]);
    let error = resolve_agent_repo_with(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        RepoResolveOptions::interactive(false),
        || Ok(false), // user declines
    )
    .await
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("cached role repo remote mismatch")
    );
    // The cached repo directory should still exist
    assert!(repo_dir.join(".git").is_dir());
}

#[tokio::test]
async fn resolve_agent_repo_rejects_cached_repo_with_local_changes() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let mut runner = FakeRunner::with_capture_queue([
        "git@github.com:jackin-project/jackin-agent-smith.git".to_owned(),
        "?? scratch.txt".to_owned(),
    ]);
    let error = resolve_agent_repo(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        false,
        None,
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("contains local changes"));
}

#[tokio::test]
async fn resolve_agent_repo_uses_run_for_clone_after_recovery() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let mut runner =
        FakeRunner::with_capture_queue(["git@github.com:evil/agent-smith.git".to_owned()]);
    let repo_dir_clone = repo_dir;
    runner.side_effects.push((
        "clone".to_owned(),
        Box::new(move || {
            std::fs::create_dir_all(repo_dir_clone.join(".git")).unwrap();
            std::fs::write(
                repo_dir_clone.join("Dockerfile"),
                "FROM projectjackin/construct:0.1-trixie\n",
            )
            .unwrap();
            std::fs::write(
                repo_dir_clone.join("jackin.role.toml"),
                r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
            )
            .unwrap();
        }),
    ));

    let result = resolve_agent_repo_with(
        &paths,
        &selector,
        "https://github.com/jackin-project/jackin-agent-smith.git",
        &mut runner,
        RepoResolveOptions::interactive(false),
        || Ok(true),
    )
    .await;

    result.expect("expected recovery to succeed");
    assert!(runner.run_recorded.iter().any(|call| {
        call.contains("git clone https://github.com/jackin-project/jackin-agent-smith.git")
    }));
}
