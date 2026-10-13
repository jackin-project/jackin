// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}

pub(super) fn workspace_file_contents(paths: &JackinPaths, name: &str) -> String {
    std::fs::read_to_string(paths.workspaces_dir.join(format!("{name}.toml"))).unwrap()
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

pub(super) fn profile_account() -> crate::AccountConfig {
    crate::AccountConfig {
        enabled: true,
        name: "Work".into(),
        provider: crate::AiProvider::Anthropic,
        credential: crate::AccountCredential::Profile {
            agent: Agent::Claude,
            directory: "/home/operator/.claude-work".into(),
            xdg_roots: None,
            source_selector: None,
        },
    }
}

pub(super) fn account_workspace(source: &Path) -> WorkspaceConfig {
    WorkspaceConfig {
        workdir: "/workspace/project".into(),
        mounts: vec![MountConfig {
            src: source.display().to_string(),
            dst: "/workspace/project".into(),
            readonly: false,
            isolation: crate::MountIsolation::Shared,
        }],
        ..Default::default()
    }
}

pub(super) fn minimal_config_file(paths: &JackinPaths) {
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        format!("version = \"{}\"\n", crate::CURRENT_CONFIG_VERSION),
    )
    .unwrap();
}

pub(super) fn claude_credentials_fixture(home: &Path) {
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(
        home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"fixture"}}"#,
    )
    .unwrap();
}
