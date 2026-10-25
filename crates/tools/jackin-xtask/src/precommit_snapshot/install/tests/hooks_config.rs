// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn installer_keeps_old_hook_enabled_if_candidate_config_update_fails() {
    let (_temporary, repository, global, system) = hook_fixture();
    let generation_a = prepare_bootstrap_generation(&repository);
    let generation_id_a = generation_a.file_name().unwrap().to_string_lossy();
    let generation_id_d = "d".repeat(64);
    let generation_d = generation_a.parent().unwrap().join(&generation_id_d);
    fs::create_dir(&generation_d).unwrap();
    fs::copy(generation_a.join("owner"), generation_d.join("owner")).unwrap();
    fs::set_permissions(
        generation_d.join("owner"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fs::set_permissions(&generation_d, fs::Permissions::from_mode(0o700)).unwrap();
    let active = repository.join(".git/jackin-precommit-owner/active");
    fs::write(&active, format!("{generation_id_a}\n")).unwrap();
    fs::set_permissions(&active, fs::Permissions::from_mode(0o600)).unwrap();
    let old_owner_command = format!(
        "\"{}/owner\" run-installed-snapshot \"{}\"",
        generation_a.display(),
        generation_a.display()
    );
    let old_hook = repository.join("scripts/old-hook");
    fs::write(
        &old_hook,
        b"#!/bin/sh\nprintf '%s\\n' old-hook >> hook-events\n",
    )
    .unwrap();
    fs::set_permissions(&old_hook, fs::Permissions::from_mode(0o755)).unwrap();
    set_hook_values(
        &repository,
        &global,
        &system,
        "--local",
        &[
            ("hook.hk-pre-commit.enabled", "true"),
            ("hook.hk-pre-commit.command", "sh scripts/old-hook"),
            ("hook.hk-pre-commit.event", "pre-commit"),
            ("hook.jackin-pre-commit.command", old_owner_command.as_str()),
            ("hook.jackin-pre-commit.event", "pre-commit"),
            ("hook.jackin-pre-commit.enabled", "true"),
        ],
    );
    fs::write(
        repository.join("scripts/hooks-native-bootstrap"),
        format!(
            "#!/bin/sh\nprintf '%s\\n' bootstrap >> bootstrap-events\nprintf '%s\\n' '{generation_id_d}' > .git/jackin-precommit-owner/active\nchmod 600 .git/jackin-precommit-owner/active\nprintf '%s\\n' \"$PWD/.git/jackin-precommit-owner/generations/{generation_id_d}\"\n"
        ),
    )
    .unwrap();
    let local_config = repository.join(".git/config");
    let local_before = fs::read(&local_config).unwrap();
    let lock_path = repository.join(".git/config.lock");
    let git = capture(Command::new("/usr/bin/which").arg("git")).unwrap();
    assert!(git.status.success());
    let real_git = String::from_utf8(git.stdout).unwrap().trim().to_owned();
    assert!(!real_git.is_empty());
    let fake_bin = repository.join("fake-bin");
    fs::create_dir(&fake_bin).unwrap();
    let fake_git = fake_bin.join("git");
    fs::write(
        &fake_git,
        format!(
            "#!/bin/sh\nfor arg in \"$@\"; do\n  if [ \"$arg\" = --file ]; then file_mode=1; fi\n  if [ \"${{file_mode:-0}}\" = 1 ] && [ \"$arg\" = hook.jackin-pre-push.event ]; then exit 73; fi\ndone\nexec \"{real_git}\" \"$@\"\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&fake_git, fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{real_git_parent}:/usr/bin:/bin",
        fake_bin.display(),
        real_git_parent = Path::new(&real_git).parent().unwrap().display()
    );

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
    assert_eq!(fs::read(local_config).unwrap(), local_before);
    assert!(!lock_path.exists());
    assert_eq!(
        fs::read_to_string(&active).unwrap(),
        format!("{generation_id_d}\n")
    );
    assert_eq!(
        git_command(
            &repository,
            &global,
            &system,
            &["config", "--local", "--get", "hook.hk-pre-commit.enabled"],
        )
        .stdout,
        b"true\n"
    );
    assert_eq!(
        git_command(
            &repository,
            &global,
            &system,
            &[
                "config",
                "--local",
                "--get",
                "hook.jackin-pre-commit.command",
            ],
        )
        .stdout,
        format!("{old_owner_command}\n").as_bytes()
    );
    let previous_hook = git_command(
        &repository,
        &global,
        &system,
        &["hook", "run", "pre-commit"],
    );
    assert!(
        previous_hook.status.success(),
        "previous immutable generation remains runnable after active advances: {}",
        String::from_utf8_lossy(&previous_hook.stderr)
    );
    assert!(
        fs::read_to_string(repository.join("hook-events"))
            .unwrap()
            .lines()
            .any(|event| event == "snapshot")
    );
}

#[test]
fn installer_updates_worktree_config_when_enabled() {
    let (_temporary, repository, global, system) = hook_fixture();
    let generation = prepare_bootstrap_generation(&repository);
    assert!(
        git_command(
            &repository,
            &global,
            &system,
            &["config", "--local", "extensions.worktreeConfig", "true"],
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
                "--worktree",
                "hook.jackin-pre-commit.command",
                "obsolete shadow value",
            ],
        )
        .status
        .success()
    );
    let common_before = fs::read(repository.join(".git/config")).unwrap();

    let installed = install_hooks(&repository, &global, &system);
    assert!(
        installed.status.success(),
        "hooks-install: {}",
        String::from_utf8_lossy(&installed.stderr)
    );
    assert_eq!(
        fs::read(repository.join(".git/config")).unwrap(),
        common_before
    );
    let generation = generation.display();
    let command = format!("\"{generation}/owner\" run-installed-snapshot \"{generation}\"");
    for (key, value) in [
        ("hook.jackin-pre-commit.command", command.as_str()),
        ("hook.jackin-pre-commit.event", "pre-commit"),
        ("hook.jackin-pre-commit.enabled", "true"),
        ("hook.hk-pre-commit.enabled", "false"),
    ] {
        let output = git_command(
            &repository,
            &global,
            &system,
            &["config", "--worktree", "--get", key],
        );
        assert!(output.status.success(), "missing worktree key {key}");
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), value);
    }
    let hooks = git_command(
        &repository,
        &global,
        &system,
        &["hook", "list", "pre-commit"],
    );
    assert!(hooks.status.success());
    let hooks = String::from_utf8_lossy(&hooks.stdout);
    assert!(hooks.lines().any(|line| line == "jackin-pre-commit"));
    assert!(!hooks.lines().any(|line| line == "hk-pre-commit"));
}
