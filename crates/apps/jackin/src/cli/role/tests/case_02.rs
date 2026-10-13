// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn parses_hardline_inspect_flag() {
    let cli = Cli::try_parse_from(["jackin", "hardline", "--inspect", "k7p9m2xq"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Hardline(HardlineArgs {
            selector: Some(ref s),
            inspect: true,
            new: false,
            agent: None,
            shell: false,
        })) if s == "k7p9m2xq"
    ));
}

#[test]
fn parses_hardline_new_agent_flags() {
    let cli = Cli::try_parse_from(["jackin", "hardline", "--new", "--agent", "codex"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Hardline(HardlineArgs {
            selector: None,
            inspect: false,
            new: true,
            agent: Some(jackin_core::Agent::Codex),
            shell: false,
        }))
    ));
}

#[test]
fn parses_hardline_shell_flag() {
    let cli = Cli::try_parse_from(["jackin", "hardline", "--shell"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Hardline(HardlineArgs {
            selector: None,
            inspect: false,
            new: false,
            agent: None,
            shell: true,
        }))
    ));
}

#[test]
fn rejects_hardline_shell_with_new() {
    let res = Cli::try_parse_from(["jackin", "hardline", "--shell", "--new"]);
    res.unwrap_err();
}

#[test]
fn rejects_hardline_shell_with_inspect() {
    let res = Cli::try_parse_from(["jackin", "hardline", "--shell", "--inspect"]);
    res.unwrap_err();
}

#[test]
fn rejects_hardline_agent_without_new() {
    let res = Cli::try_parse_from(["jackin", "hardline", "--agent", "codex"]);

    res.unwrap_err();
}

#[test]
fn rejects_hardline_inspect_with_new() {
    let res = Cli::try_parse_from(["jackin", "hardline", "--inspect", "--new"]);

    res.unwrap_err();
}

#[test]
fn parses_role_validate_with_default_path() {
    let cli = Cli::try_parse_from(["jackin", "role", "validate"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Role(RoleCommand::Validate(RoleRepoPathArgs {
            path: None
        })))
    ));
}

#[test]
fn parses_role_migrate_with_path() {
    let cli = Cli::try_parse_from(["jackin", "role", "migrate", "/tmp/my-role"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Role(RoleCommand::Migrate(
            RoleRepoPathArgs { path: Some(ref path) }
        ))) if path == std::path::Path::new("/tmp/my-role")
    ));
}

#[test]
fn parses_role_construct_version_with_default_path() {
    let cli = Cli::try_parse_from(["jackin", "role", "construct-version"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Role(RoleCommand::ConstructVersion(
            RoleRepoPathArgs { path: None }
        )))
    ));
}

#[test]
fn parses_role_construct_version_with_path() {
    let cli = Cli::try_parse_from(["jackin", "role", "construct-version", "/tmp/my-role"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Role(RoleCommand::ConstructVersion(
            RoleRepoPathArgs { path: Some(ref p) }
        ))) if p == std::path::Path::new("/tmp/my-role")
    ));
}

#[test]
fn parses_role_published_image_repository_with_path() {
    let cli = Cli::try_parse_from([
        "jackin",
        "role",
        "published-image-repository",
        "/tmp/my-role",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Role(RoleCommand::PublishedImageRepository(
            RoleRepoPathArgs { path: Some(ref p) }
        ))) if p == std::path::Path::new("/tmp/my-role")
    ));
}

#[test]
fn parses_role_publish_labels_with_path() {
    let cli = Cli::try_parse_from([
        "jackin",
        "role",
        "publish-labels",
        "--role-git-sha",
        "abc123",
        "/tmp/my-role",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Role(RoleCommand::PublishLabels(
            RolePublishLabelsArgs {
                ref role_git_sha,
                path: Some(ref p),
            }
        ))) if role_git_sha == "abc123" && p == std::path::Path::new("/tmp/my-role")
    ));
}

#[test]
fn parses_role_create_with_projects_dir() {
    let cli = Cli::try_parse_from(["jackin", "role", "create", "ChainArgos/Backend", "."]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Role(RoleCommand::Create(
            RoleCreateArgs {
                ref role,
                projects_dir: Some(ref path),
            }
        ))) if role == "ChainArgos/Backend" && path == std::path::Path::new(".")
    ));
}
