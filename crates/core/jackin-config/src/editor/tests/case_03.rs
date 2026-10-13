// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn save_rejects_reserved_name_candidate_and_preserves_on_disk_config() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    std::fs::write(&paths.config_file, "[env]\nVALID_KEY = \"v\"\n").unwrap();
    AppConfig::load_or_init(&paths).unwrap();
    let baseline = std::fs::read_to_string(&paths.config_file).unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    // Bypass the CLI pre-flight via the unchecked setter.
    editor
        .set_env_var(&EnvScope::Global, "DOCKER_HOST", "tcp://bad".into())
        .unwrap();

    let err = editor.save().unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("DOCKER_HOST") && msg.contains("reserved"),
        "expected reserved-name rejection; got: {msg}"
    );

    let after = std::fs::read_to_string(&paths.config_file).unwrap();
    assert_eq!(
        after, baseline,
        "rejected save must not touch on-disk config"
    );
}

#[test]
fn editor_save_atomic_staging_failure_preserves_every_original_file() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\nGLOBAL = \"before\"\n").unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    let workspace_path = paths.workspaces_dir.join("prod.toml");
    std::fs::write(
        &workspace_path,
        "workdir = \"/workspace/prod\"\n\n[[mounts]]\nsrc = \"/workspace/prod\"\ndst = \"/workspace/prod\"\n",
    )
    .unwrap();

    AppConfig::load_or_init(&paths).unwrap();
    let global_before = std::fs::read(&paths.config_file).unwrap();
    let workspace_before = std::fs::read(&workspace_path).unwrap();
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(&EnvScope::Global, "GLOBAL", "after".into())
        .unwrap();
    editor
        .set_env_var(
            &EnvScope::Workspace("prod".to_owned()),
            "LOCAL",
            "after".into(),
        )
        .unwrap();

    let mut stage_number = 0;
    let err = editor
        .save_with_stager(|path, contents| {
            stage_number += 1;
            if stage_number == 2 {
                return Err(std::io::Error::other("injected second-stage failure").into());
            }
            stage_atomic_write(path, contents)
        })
        .unwrap_err();
    assert!(err.to_string().contains("injected second-stage failure"));
    assert_eq!(std::fs::read(&paths.config_file).unwrap(), global_before);
    assert_eq!(std::fs::read(&workspace_path).unwrap(), workspace_before);
    let staged_leaks: Vec<_> = std::fs::read_dir(&paths.config_dir)
        .unwrap()
        .chain(std::fs::read_dir(&paths.workspaces_dir).unwrap())
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp."))
        .collect();
    assert!(
        staged_leaks.is_empty(),
        "leftover staged files: {staged_leaks:?}"
    );
}

#[test]
fn editor_save_second_commit_failure_rolls_back_every_original_file() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        "# global comment\n[env]\nGLOBAL = \"before\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    let workspace_path = paths.workspaces_dir.join("prod.toml");
    std::fs::write(
        &workspace_path,
        "# workspace comment\nworkdir = \"/workspace/prod\"\n\n[[mounts]]\nsrc = \"/workspace/prod\"\ndst = \"/workspace/prod\"\n",
    )
    .unwrap();

    AppConfig::load_or_init(&paths).unwrap();
    let global_before = std::fs::read(&paths.config_file).unwrap();
    let workspace_before = std::fs::read(&workspace_path).unwrap();
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(&EnvScope::Global, "GLOBAL", "after".into())
        .unwrap();
    editor
        .set_env_var(
            &EnvScope::Workspace("prod".to_owned()),
            "LOCAL",
            "after".into(),
        )
        .unwrap();

    // Fail the workspace rename after the global rename already committed:
    // once the workspace file is staged (second stage call), swap its
    // target for a non-empty directory so the rename fails and the
    // transaction must restore the already-committed global file.
    // Rollback restores bytes, not modes (restore re-stages through a
    // fresh 0o600 temp file), so only contents are asserted below.
    let mut stage_number = 0;
    let err = editor
        .save_with_stager(|path, contents| {
            let staged = stage_atomic_write(path, contents)?;
            stage_number += 1;
            if stage_number == 2 {
                std::fs::remove_file(&workspace_path)?;
                std::fs::create_dir(&workspace_path)?;
                std::fs::write(workspace_path.join("blocker"), b"not a config")?;
            }
            Ok(staged)
        })
        .unwrap_err();
    assert_eq!(stage_number, 2);
    assert!(
        err.to_string().contains("renaming"),
        "expected the injected rename failure; got: {err:#}"
    );
    std::fs::remove_file(workspace_path.join("blocker")).unwrap();
    std::fs::remove_dir(&workspace_path).unwrap();
    std::fs::write(&workspace_path, &workspace_before).unwrap();
    assert_eq!(std::fs::read(&paths.config_file).unwrap(), global_before);
    assert_eq!(std::fs::read(&workspace_path).unwrap(), workspace_before);

    let leftovers: Vec<_> = std::fs::read_dir(&paths.config_dir)
        .unwrap()
        .chain(std::fs::read_dir(&paths.workspaces_dir).unwrap())
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.contains(".tmp.")
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "leftover transaction files: {leftovers:?}"
    );

    // The failed editor released its lock only after rollback completed; a
    // fresh editor can immediately publish the same tree again.
    ConfigEditor::open(&paths).unwrap().save().unwrap();
}

#[test]
fn editor_save_keeps_exclusive_lock_during_publication() {
    use std::sync::mpsc;
    use std::time::Duration;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\nGLOBAL = \"before\"\n").unwrap();
    AppConfig::load_or_init(&paths).unwrap();

    let (go_tx, go_rx) = mpsc::channel();
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let (opened_tx, opened_rx) = mpsc::channel();
    let probe_paths = paths.clone();
    let probe = std::thread::spawn(move || {
        go_rx.recv().unwrap();
        attempt_tx.send(()).unwrap();
        opened_tx
            .send(ConfigEditor::open(&probe_paths).is_ok())
            .unwrap();
    });

    let editor = ConfigEditor::open(&paths).unwrap();
    let mut stage_number = 0;
    editor
        .save_with_stager(|path, contents| {
            let staged = stage_atomic_write(path, contents)?;
            stage_number += 1;
            if stage_number == 1 {
                go_tx.send(()).unwrap();
                attempt_rx.recv().unwrap();
                assert!(
                    opened_rx.recv_timeout(Duration::from_millis(50)).is_err(),
                    "a second editor observed publication before the first released its lock"
                );
            }
            Ok(staged)
        })
        .unwrap();
    probe.join().unwrap();
    assert!(opened_rx.recv_timeout(Duration::from_secs(1)).unwrap());
}

#[test]
fn editor_save_repeated_is_byte_idempotent_for_global_and_workspace_files() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\nGLOBAL = \"value\"\n").unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    let workspace_path = paths.workspaces_dir.join("prod.toml");
    std::fs::write(
        &workspace_path,
        "workdir = \"/workspace/prod\"\n\n[[mounts]]\nsrc = \"/workspace/prod\"\ndst = \"/workspace/prod\"\n",
    )
    .unwrap();

    AppConfig::load_or_init(&paths).unwrap();
    ConfigEditor::open(&paths).unwrap().save().unwrap();
    let global_after_first_save = std::fs::read(&paths.config_file).unwrap();
    let workspace_after_first_save = std::fs::read(&workspace_path).unwrap();

    ConfigEditor::open(&paths).unwrap().save().unwrap();
    assert_eq!(
        std::fs::read(&paths.config_file).unwrap(),
        global_after_first_save
    );
    assert_eq!(
        std::fs::read(&workspace_path).unwrap(),
        workspace_after_first_save
    );
}

#[test]
fn editor_save_rejects_invalid_workspace_stem_before_any_write() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "[env]\nSAFE = \"before\"\n").unwrap();
    AppConfig::load_or_init(&paths).unwrap();
    let before = std::fs::read(&paths.config_file).unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .set_env_var(
            &EnvScope::Workspace("../escape".to_owned()),
            "VALUE",
            "after".into(),
        )
        .unwrap();
    editor.save().unwrap_err();
    assert_eq!(std::fs::read(&paths.config_file).unwrap(), before);
    assert!(!paths.config_dir.join("escape.toml").exists());
}

#[test]
fn add_mount_unscoped_creates_single_mount_entry() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.add_mount(
        "shared-home",
        MountConfig {
            src: "/home/user".to_owned(),
            dst: "/workspace/home".to_owned(),
            readonly: false,
            isolation: crate::MountIsolation::Shared,
        },
        None,
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(out.contains("[docker.mounts.shared-home]"), "{out}");
    assert!(out.contains(r#"src = "/home/user""#), "{out}");
}

#[test]
fn add_mount_scoped_creates_nested_entry() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    // Behavioral equivalence with AppConfig::add_mount:
    // scope is the OUTER key; name is the INNER key.
    // So scope=agent-smith produces [docker.mounts.agent-smith] with creds = {...}
    editor.add_mount(
        "creds",
        MountConfig {
            src: "/run/secrets/x".to_owned(),
            dst: "/secrets/x".to_owned(),
            readonly: true,
            isolation: crate::MountIsolation::Shared,
        },
        Some("agent-smith"),
    );
    editor.save().unwrap();

    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    // The scoped shape: [docker.mounts.agent-smith] with creds sub-table
    assert!(out.contains("[docker.mounts.agent-smith]"), "{out}");
    assert!(out.contains(r#"src = "/run/secrets/x""#), "{out}");
    assert!(out.contains("readonly = true"), "{out}");
}

#[test]
fn remove_mount_unscoped_deletes_entry() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[docker.mounts.shared-home]
src = "/home/user"
dst = "/workspace/home"
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let removed = editor.remove_mount("shared-home", None);
    editor.save().unwrap();

    assert!(removed);
    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!out.contains("shared-home"), "{out}");
}

#[test]
fn remove_mount_returns_false_for_missing() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, "").unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let removed = editor.remove_mount("nope", None);
    editor.save().unwrap();
    assert!(!removed);
}

#[test]
fn remove_mount_scoped_last_entry_deletes_scope_table() {
    // Matches AppConfig::remove_mount cleanup: when the last named mount
    // in a scope is removed, the scope table itself is removed.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[docker.mounts.agent-smith]
creds = { src = "/run/secrets/x", dst = "/secrets/x" }
"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let removed = editor.remove_mount("creds", Some("agent-smith"));
    editor.save().unwrap();

    assert!(removed);
    let out = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        !out.contains("agent-smith"),
        "empty scope table should be gone: {out}"
    );
}
