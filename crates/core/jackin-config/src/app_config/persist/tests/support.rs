// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn wait_for_mtime_tick() {
    #[expect(
        clippy::disallowed_methods,
        reason = "mtime idempotency test needs a wall-clock boundary before checking no rewrite"
    )]
    std::thread::sleep(std::time::Duration::from_millis(50));
}

pub(super) fn workspace_tree_bytes(paths: &JackinPaths) -> Option<Vec<(String, Vec<u8>)>> {
    let entries = match std::fs::read_dir(&paths.workspaces_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => panic!("reading workspace tree: {error}"),
    };
    let mut files = entries
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                std::fs::read(entry.path()).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Some(files)
}

pub(super) fn assert_no_staged_writes(paths: &JackinPaths) {
    for directory in [&paths.config_dir, &paths.workspaces_dir] {
        let entries = std::fs::read_dir(directory).unwrap();
        for entry in entries {
            let entry = entry.unwrap();
            assert!(
                !entry.file_name().to_string_lossy().contains(".tmp."),
                "staged file leaked: {}",
                entry.path().display()
            );
        }
    }
}

pub(super) fn disc_workspace_toml(version: &str, workdir: &str) -> String {
    format!(
        "version = \"{version}\"\nworkdir = \"{workdir}\"\n\n[[mounts]]\nsrc = \"/host/source\"\ndst = \"{workdir}\"\n"
    )
}

pub(super) fn disc_write_tree(paths: &JackinPaths) {
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
    std::fs::write(
        &paths.config_file,
        format!("version = \"{CURRENT_CONFIG_VERSION}\"\n\n[github]\nauth_forward = \"sync\"\n"),
    )
    .unwrap();
    std::fs::write(
        paths.workspaces_dir.join("alpha.toml"),
        disc_workspace_toml(CURRENT_WORKSPACE_VERSION, "/workspace/alpha"),
    )
    .unwrap();
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct DiscFileStamp {
    bytes: Vec<u8>,
    len: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    mode: u32,
}

pub(super) fn disc_file_stamp(path: &Path) -> DiscFileStamp {
    let metadata = std::fs::metadata(path).unwrap();
    DiscFileStamp {
        bytes: std::fs::read(path).unwrap(),
        len: metadata.len(),
        modified: metadata.modified().unwrap(),
        #[cfg(unix)]
        mode: {
            use std::os::unix::fs::PermissionsExt as _;
            metadata.permissions().mode()
        },
    }
}

pub(super) fn disc_dir_entries(path: &Path) -> Vec<String> {
    let mut entries = std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}
