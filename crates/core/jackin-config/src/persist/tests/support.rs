// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn journal_workspace_toml(workdir: &str) -> String {
    format!(
        "version = \"{}\"\nworkdir = \"{workdir}\"\n\n[[mounts]]\nsrc = \"/host/source\"\ndst = \"{workdir}\"\n",
        crate::CURRENT_WORKSPACE_VERSION
    )
}

pub(super) fn journal_global_with_marker(global_before: &str) -> String {
    let mut doc: toml_edit::DocumentMut = global_before.parse().unwrap();
    doc["env"]["JOURNAL_RECOVERED"] = toml_edit::value("yes");
    doc.to_string()
}

pub(super) fn assert_no_staged_files(paths: &JackinPaths) {
    for dir in [&paths.config_dir, &paths.workspaces_dir] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.map(|entry| entry.unwrap()) {
            assert!(
                !entry.file_name().to_string_lossy().contains(".tmp."),
                "staged file leaked: {}",
                entry.path().display()
            );
        }
    }
}

pub(super) fn staged_config_tree() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let workspace = temp.path().join("workspaces").join("ws.toml");
    std::fs::create_dir_all(workspace.parent().unwrap()).unwrap();
    std::fs::write(&config, "global-old").unwrap();
    std::fs::write(&workspace, "ws-old").unwrap();
    (temp, config, workspace)
}

pub(super) fn staged_leftovers(dir: &Path) -> Vec<PathBuf> {
    let mut leftovers = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains(".tmp."))
        {
            leftovers.push(path);
        }
    }
    leftovers
}

pub(super) fn injected_failure() -> crate::ConfigError {
    crate::ConfigError::msg(format_args!("injected transaction failure"))
}

pub(super) fn assert_recovered_generation(config: &Path, workspace: &Path, expected: [&str; 2]) {
    let _guard = acquire_config_write_lock(config).unwrap();
    assert_eq!(std::fs::read_to_string(config).unwrap(), expected[0]);
    assert_eq!(std::fs::read_to_string(workspace).unwrap(), expected[1]);
    assert!(!publication_journal_path(config).exists());
    for directory in [config.parent().unwrap(), workspace.parent().unwrap()] {
        for entry in std::fs::read_dir(directory).unwrap() {
            assert!(!is_staged_garbage(&entry.unwrap().file_name()));
        }
    }
}
