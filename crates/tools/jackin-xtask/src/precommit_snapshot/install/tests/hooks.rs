// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn assert_installed_hook_config(
    repository: &Path,
    global: &Path,
    system: &Path,
    generation: &Path,
) {
    let generation = generation.display();
    let command = format!("\"{generation}/owner\" run-installed-snapshot \"{generation}\"");
    for (key, expected) in [
        ("hook.hk-pre-commit.enabled", "false"),
        ("hook.jackin-pre-commit.command", command.as_str()),
        ("hook.jackin-pre-commit.event", "pre-commit"),
        ("hook.jackin-pre-commit.enabled", "true"),
        ("hook.hk-pre-push.enabled", "false"),
        (
            "hook.jackin-pre-push.command",
            "sh scripts/pre-push-security",
        ),
        ("hook.jackin-pre-push.event", "pre-push"),
        ("hook.jackin-pre-push.enabled", "true"),
    ] {
        let output = git_command(
            repository,
            global,
            system,
            &["config", "--local", "--get", key],
        );
        assert!(output.status.success(), "missing {key}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            expected,
            "{key}"
        );
    }
    for key in [
        "hook.hk-pre-commit.command",
        "hook.hk-pre-commit.event",
        "hook.hk-pre-push.command",
        "hook.hk-pre-push.event",
    ] {
        let output = git_command(
            repository,
            global,
            system,
            &["config", "--local", "--get", key],
        );
        assert!(!output.status.success(), "{key} remains configured");
    }
}

#[test]
fn hook_installer_preserves_global_hooks_and_is_idempotent_in_a_fixture_repo() {
    let (_temporary, repository, global, system) = hook_fixture();
    let generation = prepare_bootstrap_generation(&repository);
    fs::write(
        repository.join("scripts/unrelated"),
        b"#!/bin/sh\nprintf '%s\\n' unrelated >> hook-events\n",
    )
    .unwrap();
    fs::write(
        repository.join("scripts/reinstalled-hk"),
        b"#!/bin/sh\nprintf '%s\\n' unsafe-hk >> hook-events\n",
    )
    .unwrap();
    set_hook_values(
        &repository,
        &global,
        &system,
        "--global",
        &[
            ("hook.hk-pre-commit.command", "sh scripts/unsafe-hk"),
            ("hook.hk-pre-commit.event", "pre-commit"),
            ("hook.other-project.command", "sh scripts/unrelated"),
            ("hook.other-project.event", "pre-commit"),
            ("hook.hk-pre-push.command", "sh scripts/unsafe-push"),
            ("hook.hk-pre-push.event", "pre-push"),
        ],
    );
    set_hook_values(
        &repository,
        &global,
        &system,
        "--local",
        &[
            ("hook.hk-pre-commit.command", "obsolete"),
            ("hook.hk-pre-commit.event", "pre-commit"),
        ],
    );
    let global_before = fs::read(&global).unwrap();

    let installed = install_hooks(&repository, &global, &system);
    assert!(
        installed.status.success(),
        "hooks-install: {}",
        String::from_utf8_lossy(&installed.stderr)
    );
    assert_eq!(fs::read(&global).unwrap(), global_before);
    assert_eq!(
        fs::read_to_string(repository.join("bootstrap-events")).unwrap(),
        "bootstrap\n"
    );
    assert_installed_hook_config(&repository, &global, &system, &generation);
    let local_after_first = fs::read(repository.join(".git/config")).unwrap();
    let installed_again = install_hooks(&repository, &global, &system);
    assert!(
        installed_again.status.success(),
        "second hooks-install: {}",
        String::from_utf8_lossy(&installed_again.stderr)
    );
    assert_eq!(
        fs::read(repository.join(".git/config")).unwrap(),
        local_after_first
    );
    assert_eq!(fs::read(&global).unwrap(), global_before);
    assert_eq!(
        fs::read_to_string(repository.join("bootstrap-events")).unwrap(),
        "bootstrap\nbootstrap\n"
    );

    assert!(
        git_command(
            &repository,
            &global,
            &system,
            &[
                "config",
                "--global",
                "--replace-all",
                "hook.hk-pre-commit.command",
                "sh scripts/reinstalled-hk"
            ]
        )
        .status
        .success()
    );
    assert!(
        git_command(
            &repository,
            &global,
            &system,
            &[
                "config",
                "--global",
                "--replace-all",
                "hook.hk-pre-commit.event",
                "pre-commit"
            ]
        )
        .status
        .success()
    );
    let hook = git_command(
        &repository,
        &global,
        &system,
        &["hook", "run", "pre-commit"],
    );
    assert!(
        hook.status.success(),
        "hook run: {}",
        String::from_utf8_lossy(&hook.stderr)
    );
    assert_eq!(
        fs::read_to_string(repository.join("hook-events")).unwrap(),
        "unrelated\nsnapshot\n"
    );
}

#[test]
fn hook_installer_rejects_failed_native_bootstrap_before_config_mutation() {
    let (_temporary, repository, global, system) = hook_fixture();
    let local_config = repository.join(".git/config");
    let local_before = fs::read(&local_config).unwrap();
    let global_before = fs::read(&global).unwrap();
    fs::write(
        repository.join("scripts/hooks-native-bootstrap"),
        b"#!/bin/sh\nprintf '%s\\n' failed >> bootstrap-events\nexit 23\n",
    )
    .unwrap();

    let result = install_hooks(&repository, &global, &system);

    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("native hook provisioning failed; hook registration was not changed")
    );
    assert_eq!(fs::read(local_config).unwrap(), local_before);
    assert_eq!(fs::read(global).unwrap(), global_before);
    assert_eq!(
        fs::read_to_string(repository.join("bootstrap-events")).unwrap(),
        "failed\n"
    );
}

#[test]
fn hook_installer_rejects_unverified_generation_path_before_config_mutation() {
    let (_temporary, repository, global, system) = hook_fixture();
    let local_config = repository.join(".git/config");
    let local_before = fs::read(&local_config).unwrap();
    let global_before = fs::read(&global).unwrap();
    fs::write(
        repository.join("scripts/hooks-native-bootstrap"),
        b"#!/bin/sh\nprintf '%s\\n' \"$PWD/.git/jackin-precommit-owner/generations/missing\"\n",
    )
    .unwrap();

    let result = install_hooks(&repository, &global, &system);

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("provisioned immutable owner"));
    assert_eq!(fs::read(local_config).unwrap(), local_before);
    assert_eq!(fs::read(global).unwrap(), global_before);
}

#[test]
fn hook_installer_rejects_unsupported_git_without_config_mutation() {
    let (_temporary, repository, global, system) = hook_fixture();
    let _generation = prepare_bootstrap_generation(&repository);
    let local_config = repository.join(".git/config");
    let before = fs::read(&local_config).unwrap();
    let mut find_git = Command::new("/usr/bin/which");
    let real_git = capture(find_git.arg("git")).expect("find git");
    assert!(real_git.status.success());
    let real_git = String::from_utf8(real_git.stdout)
        .unwrap()
        .trim()
        .to_owned();
    let fake_bin = repository.join("fake-bin");
    fs::create_dir(&fake_bin).unwrap();
    let fake_git = fake_bin.join("git");
    fs::write(
            &fake_git,
            format!(
                "#!/bin/sh\nfor arg in \"$@\"; do [ \"$arg\" = hook ] && exit 129; done\nexec \"{real_git}\" \"$@\"\n"
            ),
        )
        .unwrap();
    fs::set_permissions(&fake_git, fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:/usr/bin:/bin", fake_bin.display());
    let mut command = Command::new("sh");
    command
        .arg("scripts/hooks-install")
        .current_dir(&repository)
        .env("PATH", path)
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("GIT_CONFIG_SYSTEM", &system)
        .env("GIT_CONFIG_NOSYSTEM", "1");
    let result = capture(&mut command).unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(local_config).unwrap(), before);
}
