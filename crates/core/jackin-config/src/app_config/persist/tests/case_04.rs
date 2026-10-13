// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn disc_read_only_leaves_bytes_metadata_permissions_and_entries_unchanged() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    disc_write_tree(&paths);
    let workspace_file = paths.workspaces_dir.join("alpha.toml");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&paths.config_file, std::fs::Permissions::from_mode(0o400))
            .unwrap();
        std::fs::set_permissions(&workspace_file, std::fs::Permissions::from_mode(0o400)).unwrap();
        std::fs::set_permissions(
            &paths.workspaces_dir,
            std::fs::Permissions::from_mode(0o500),
        )
        .unwrap();
        std::fs::set_permissions(&paths.config_dir, std::fs::Permissions::from_mode(0o500))
            .unwrap();
    }

    let config_before = disc_file_stamp(&paths.config_file);
    let workspace_before = disc_file_stamp(&workspace_file);
    let config_entries_before = disc_dir_entries(&paths.config_dir);
    let workspace_entries_before = disc_dir_entries(&paths.workspaces_dir);

    let snapshot = load_read_only_config_snapshot(&paths).unwrap();

    assert!(
        snapshot.diagnostics.is_empty(),
        "{:?}",
        snapshot.diagnostics
    );
    assert_eq!(disc_file_stamp(&paths.config_file), config_before);
    assert_eq!(disc_file_stamp(&workspace_file), workspace_before);
    assert_eq!(disc_dir_entries(&paths.config_dir), config_entries_before);
    assert_eq!(
        disc_dir_entries(&paths.workspaces_dir),
        workspace_entries_before
    );
    assert!(!paths.config_file.with_file_name("config.lock").exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&paths.config_dir, std::fs::Permissions::from_mode(0o700))
            .unwrap();
        std::fs::set_permissions(
            &paths.workspaces_dir,
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
    }
}

#[test]
fn disc_read_only_generation_is_content_based() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    disc_write_tree(&paths);
    let first = load_read_only_config_snapshot(&paths).unwrap();
    let bytes = std::fs::read(&paths.config_file).unwrap();
    std::fs::write(&paths.config_file, bytes).unwrap();

    let second = load_read_only_config_snapshot(&paths).unwrap();

    assert_eq!(first.generation, second.generation);
    assert_eq!(first.generation.as_str().len(), 64);
}

#[test]
fn disc_read_only_repeated_torn_tree_returns_only_transient_diagnostic() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    disc_write_tree(&paths);

    let snapshot = load_read_only_config_snapshot_with_hook(&paths, |attempt| {
        std::fs::write(
            &paths.config_file,
            format!(
                "version = \"{CURRENT_CONFIG_VERSION}\"\nrole_repo_refresh_ttl_seconds = {}\n",
                attempt + 1
            ),
        )
        .unwrap();
    })
    .unwrap();

    assert!(snapshot.config.workspaces.is_empty());
    assert_eq!(
        snapshot.diagnostics,
        vec![ConfigSourceDiagnostic {
            scope: ConfigSourceScope::Workspaces,
            issue: ConfigSourceIssue::TransientConflict,
        }]
    );
}

#[test]
fn disc_read_only_pending_publication_reports_transient_without_mutation() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    disc_write_tree(&paths);

    // Simulate kill -9 mid-publication: journal plus staged tmps on disk.
    let journal_path = publication_journal_path(&paths.config_file);
    let staged = vec![stage_atomic_write(&paths.config_file, "version = \"v9alpha9\"\n").unwrap()];
    let deletes: Vec<StagedDelete> = Vec::new();
    write_publication_journal(&journal_path, &staged, &deletes).unwrap();
    leak_staged_writes(staged);

    let workspace_file = paths.workspaces_dir.join("alpha.toml");
    let config_before = disc_file_stamp(&paths.config_file);
    let workspace_before = disc_file_stamp(&workspace_file);
    let journal_before = disc_file_stamp(&journal_path);
    let config_entries_before = disc_dir_entries(&paths.config_dir);
    let workspace_entries_before = disc_dir_entries(&paths.workspaces_dir);

    let snapshot = load_read_only_config_snapshot(&paths).unwrap();

    // Skewed bytes are never served as a stable generation.
    assert!(snapshot.config.workspaces.is_empty());
    assert_eq!(
        snapshot.diagnostics,
        vec![ConfigSourceDiagnostic {
            scope: ConfigSourceScope::Workspaces,
            issue: ConfigSourceIssue::TransientConflict,
        }]
    );
    assert_eq!(disc_file_stamp(&paths.config_file), config_before);
    assert_eq!(disc_file_stamp(&workspace_file), workspace_before);
    assert_eq!(disc_file_stamp(&journal_path), journal_before);
    assert_eq!(disc_dir_entries(&paths.config_dir), config_entries_before);
    assert_eq!(
        disc_dir_entries(&paths.workspaces_dir),
        workspace_entries_before
    );
    assert!(!paths.config_file.with_file_name("config.lock").exists());
}
