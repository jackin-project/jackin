// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::hooks::assert_installed_hook_config;
use super::*;

fn linked_worktree_fixture() -> (
    tempfile::TempDir,
    PathBuf,
    PathBuf,
    PathBuf,
    PathBuf,
    PathBuf,
) {
    let (temporary, primary, global, system) = hook_fixture();
    assert!(
        git_command(&primary, &global, &system, &["add", "scripts"])
            .status
            .success()
    );
    assert!(
        git_command(
            &primary,
            &global,
            &system,
            &["commit", "-q", "-m", "linked worktree fixture"],
        )
        .status
        .success()
    );
    let linked = temporary.path().join("linked");
    assert!(
        git_command(
            &primary,
            &global,
            &system,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "linked-fixture",
                linked.to_str().unwrap(),
            ],
        )
        .status
        .success()
    );
    assert!(
        git_command(
            &linked,
            &global,
            &system,
            &["config", "--worktree", "jackin.fixtureScope", "true"],
        )
        .status
        .success()
    );
    let common = git_path(
        &primary,
        &global,
        &system,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    );
    (temporary, primary, linked, global, system, common)
}

fn git_path(repository: &Path, global: &Path, system: &Path, args: &[&str]) -> PathBuf {
    let output = git_command(repository, global, system, args);
    assert!(
        output.status.success(),
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
}

fn set_generation_bootstrap(repository: &Path, common: &Path, generation_id: &str) -> PathBuf {
    let generation = common
        .join("jackin-precommit-owner/generations")
        .join(generation_id);
    fs::create_dir_all(&generation).unwrap();
    fs::write(
        generation.join("owner"),
        b"#!/bin/sh\nset -eu\ncase \"$1\" in\nvalidate-hook-config) exit 0 ;;\nactivate-hook-config) config=$(git -C \"$3\" rev-parse --path-format=absolute --git-path config.worktree); mv \"$config.lock\" \"$config\" ;;\n*) exit 64 ;;\nesac\n",
    )
    .unwrap();
    fs::set_permissions(generation.join("owner"), fs::Permissions::from_mode(0o755)).unwrap();
    let bootstrap = format!(
        "#!/bin/sh\nset -eu\nprintf '%s\\n' '{}'\n",
        generation.display()
    );
    fs::write(repository.join("scripts/hooks-native-bootstrap"), bootstrap).unwrap();
    fs::set_permissions(
        repository.join("scripts/hooks-native-bootstrap"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    generation
}

#[test]
fn linked_worktree_installs_hooks_without_changing_primary_registration() {
    let (_temporary, primary, linked, global, system, common) = linked_worktree_fixture();
    let primary_generation = set_generation_bootstrap(&primary, &common, &"a".repeat(64));
    let linked_generation = set_generation_bootstrap(&linked, &common, &"b".repeat(64));
    let common_config = common.join("config");
    let primary_config = git_path(
        &primary,
        &global,
        &system,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "config.worktree",
        ],
    );
    let linked_config = git_path(
        &linked,
        &global,
        &system,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "config.worktree",
        ],
    );
    let common_before = fs::read(&common_config).unwrap();
    let global_before = fs::read(&global).unwrap();

    let primary_install = install_hooks(&primary, &global, &system);
    assert!(
        primary_install.status.success(),
        "primary hooks-install: {}",
        String::from_utf8_lossy(&primary_install.stderr)
    );
    let primary_after = fs::read(&primary_config).unwrap();
    assert_installed_hook_config(&primary, &global, &system, &primary_generation);

    let linked_install = install_hooks(&linked, &global, &system);
    assert!(
        linked_install.status.success(),
        "linked hooks-install: {}",
        String::from_utf8_lossy(&linked_install.stderr)
    );
    assert_eq!(fs::read(&primary_config).unwrap(), primary_after);
    assert_eq!(fs::read(&common_config).unwrap(), common_before);
    assert_eq!(fs::read(&global).unwrap(), global_before);
    assert_installed_hook_config(&primary, &global, &system, &primary_generation);
    assert_installed_hook_config(&linked, &global, &system, &linked_generation);
    assert_ne!(fs::read(&linked_config).unwrap(), primary_after);
}

#[test]
fn hook_installer_refuses_disabled_worktree_config_in_primary_and_linked_worktrees() {
    let (_temporary, primary, linked, global, system, common) = linked_worktree_fixture();
    let common_config = common.join("config");
    let primary_config = git_path(
        &primary,
        &global,
        &system,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "config.worktree",
        ],
    );
    let linked_config = git_path(
        &linked,
        &global,
        &system,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "config.worktree",
        ],
    );
    let worktrees = [&primary, &linked];

    for extension_setting in [Some("false"), None] {
        if let Some(setting) = extension_setting {
            assert!(
                git_command(
                    &primary,
                    &global,
                    &system,
                    &["config", "--local", "extensions.worktreeConfig", setting],
                )
                .status
                .success()
            );
        } else {
            assert!(
                git_command(
                    &primary,
                    &global,
                    &system,
                    &["config", "--local", "--unset", "extensions.worktreeConfig"],
                )
                .status
                .success()
            );
        }
        let common_before = fs::read(&common_config).unwrap();
        let primary_before = fs::read(&primary_config).unwrap();
        let linked_before = fs::read(&linked_config).unwrap();
        let global_before = fs::read(&global).unwrap();

        for repository in worktrees {
            let bootstrap_events = repository.join("bootstrap-events");
            assert!(!bootstrap_events.exists());
            let result = install_hooks(repository, &global, &system);
            assert!(!result.status.success());
            assert!(
                String::from_utf8_lossy(&result.stderr)
                    .contains("extensions.worktreeConfig must be enabled")
            );
            assert!(!bootstrap_events.exists(), "bootstrap ran before refusal");
            assert_eq!(fs::read(&common_config).unwrap(), common_before);
            assert_eq!(fs::read(&primary_config).unwrap(), primary_before);
            assert_eq!(fs::read(&linked_config).unwrap(), linked_before);
            assert_eq!(fs::read(&global).unwrap(), global_before);
        }
        assert!(
            git_command(
                &primary,
                &global,
                &system,
                &["config", "--local", "extensions.worktreeConfig", "true"],
            )
            .status
            .success()
        );
    }
}
