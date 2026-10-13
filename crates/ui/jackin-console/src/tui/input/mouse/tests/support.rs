// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn list_state() -> ManagerState<'static> {
    let config = jackin_config::AppConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    ManagerState::from_config(&config, tmp.path())
}

pub(super) fn file_browser_with_dirs(
    root: &std::path::Path,
    count: usize,
) -> crate::tui::components::file_browser::FileBrowserState {
    for i in 0..count {
        std::fs::create_dir_all(root.join(format!("dir-{i}"))).unwrap();
    }
    crate::tui::components::file_browser::FileBrowserState::from_listing(
        crate::services::file_browser::listing_at(root.to_path_buf(), root.to_path_buf()),
    )
}

pub(super) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

pub(super) const fn mouse(kind: MouseEventKind, col: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: col,
        row: 0,
        modifiers: KeyModifiers::NONE,
    }
}

pub(super) const fn term(width: u16) -> Rect {
    Rect {
        x: 0,
        y: 0,
        width,
        height: 30,
    }
}

pub(super) fn term_120x40() -> Rect {
    Rect {
        x: 0,
        y: 0,
        width: 120,
        height: 40,
    }
}

pub(super) const fn mouse_down_at(col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

pub(super) const fn mouse_at(col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

pub(super) const fn mouse_kind_at(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

pub(super) fn list_state_with_saved(n: usize) -> ManagerState<'static> {
    let mut config = jackin_config::AppConfig::default();
    for i in 0..n {
        config.workspaces.insert(
            format!("ws-{i:02}"),
            WorkspaceConfig {
                workdir: format!("/w/{i}"),
                mounts: vec![],
                ..Default::default()
            },
        );
    }
    let tmp = tempfile::tempdir().unwrap();
    ManagerState::from_config(&config, tmp.path())
}

pub(super) fn config_with_scrollable_workspace_and_global_mounts() -> jackin_config::AppConfig {
    let mut config = jackin_config::AppConfig::default();
    config.workspaces.insert(
            "demo".into(),
            WorkspaceConfig {
                workdir: "/workspace/demo".into(),
                mounts: vec![MountConfig {
                    src: "/host/source/with/a/very/long/path/that/forces/workspace/mount/scrolling"
                        .into(),
                    dst: "/container/destination/with/a/very/long/path/that/forces/workspace/mount/scrolling"
                        .into(),
                    readonly: false,
                    isolation: jackin_config::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        );
    config.add_mount(
        "global-long",
        MountConfig {
            src: "/host/source/with/a/very/long/path/that/forces/global/mount/scrolling".into(),
            dst: "/container/destination/with/a/very/long/path/that/forces/global/mount/scrolling"
                .into(),
            readonly: true,
            isolation: jackin_config::MountIsolation::Shared,
        },
        None,
    );
    config
}

pub(super) fn selected_demo_state(config: &jackin_config::AppConfig) -> ManagerState<'static> {
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(config, tmp.path());
    state.selected = 1;
    state
}

pub(super) fn current_dir_state_at(path: &std::path::Path) -> ManagerState<'static> {
    let config = jackin_config::AppConfig::default();
    ManagerState::from_config(&config, path)
}

pub(super) fn config_with_long_git_type_mount(
    source: &std::path::Path,
) -> jackin_config::AppConfig {
    let mut config = jackin_config::AppConfig::default();
    config.workspaces.insert(
        "demo".into(),
        WorkspaceConfig {
            workdir: "/workspace/demo".into(),
            mounts: vec![MountConfig {
                src: source.display().to_string(),
                dst: source.display().to_string(),
                readonly: false,
                isolation: jackin_config::MountIsolation::Shared,
            }],
            ..Default::default()
        },
    );
    config
}
