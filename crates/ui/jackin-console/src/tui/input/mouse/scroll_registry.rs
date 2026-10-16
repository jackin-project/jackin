// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::layout::Rect;

use crate::tui::state::ManagerState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleScrollBlock {
    ListNames,
    ListWorkspaceMounts,
    ListGlobalMounts,
    ListRoleGlobalMounts,
    ListRoles,
    EditorTabContent,
    EditorWorkspaceMounts,
    SettingsMounts,
    SettingsEnv,
    SettingsTrust,
    SettingsAuth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollBlockRegion {
    pub id: ConsoleScrollBlock,
    pub rect: Rect,
    pub content_w: usize,
    pub content_h: usize,
}

impl ScrollBlockRegion {
    #[must_use]
    pub const fn hit_region(&self) -> termrock::interaction::HitRegion<ConsoleScrollBlock> {
        termrock::interaction::HitRegion {
            id: self.id,
            area: self.rect,
        }
    }
}

#[must_use]
pub fn hit(blocks: &[ScrollBlockRegion], col: u16, row: u16) -> Option<ConsoleScrollBlock> {
    let position = ratatui::layout::Position::new(col, row);
    blocks
        .iter()
        .rev()
        .find(|block| block.rect.contains(position))
        .map(|block| block.id)
}

#[must_use]
pub fn scroll_block_registry(
    state: &ManagerState<'_>,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) -> Vec<ScrollBlockRegion> {
    match &state.stage {
        crate::tui::state::ManagerStage::List => list_blocks(state, term_size, config),
        crate::tui::state::ManagerStage::Editor(editor) => editor_blocks(editor, term_size),
        crate::tui::state::ManagerStage::Settings(settings) => settings_blocks(settings, term_size),
        crate::tui::state::ManagerStage::CreatePrelude(_)
        | crate::tui::state::ManagerStage::Workflows(_)
        | crate::tui::state::ManagerStage::ConfirmDelete { .. }
        | crate::tui::state::ManagerStage::ConfirmInstancePurge { .. } => Vec::new(),
    }
}

fn list_blocks(
    state: &ManagerState<'_>,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) -> Vec<ScrollBlockRegion> {
    let mut blocks = Vec::new();
    let (left_x, left_w, _, _) =
        crate::tui::layout::horizontal_split_pane_dims(state.list_split_pct, term_size.width);
    let names_area = Rect {
        x: left_x,
        y: super::LIST_HEADER_HEIGHT,
        width: left_w,
        height: term_size
            .height
            .saturating_sub(super::LIST_HEADER_HEIGHT + super::LIST_FOOTER_HEIGHT),
    };
    let names_viewport = crate::tui::layout::scroll_viewport_width(names_area);
    blocks.push(ScrollBlockRegion {
        id: ConsoleScrollBlock::ListNames,
        rect: names_area,
        content_w: crate::tui::layout::list::list_names_content_width(state, names_viewport),
        content_h: 0,
    });
    if let Some(areas) = super::list_scroll_areas(state, term_size, config) {
        blocks.push(ScrollBlockRegion {
            id: ConsoleScrollBlock::ListWorkspaceMounts,
            rect: areas.workspace.area,
            content_w: areas.workspace.content_width,
            content_h: areas.workspace.content_height,
        });
        blocks.push(ScrollBlockRegion {
            id: ConsoleScrollBlock::ListGlobalMounts,
            rect: areas.global.area,
            content_w: areas.global.content_width,
            content_h: areas.global.content_height,
        });
        if let Some(role_global) = areas.role_global {
            blocks.push(ScrollBlockRegion {
                id: ConsoleScrollBlock::ListRoleGlobalMounts,
                rect: role_global.area,
                content_w: role_global.content_width,
                content_h: role_global.content_height,
            });
        }
        if let Some(roles) = areas.roles {
            blocks.push(ScrollBlockRegion {
                id: ConsoleScrollBlock::ListRoles,
                rect: roles.area,
                content_w: roles.content_width,
                content_h: roles.content_height,
            });
        }
    }
    blocks
}

fn editor_blocks(
    editor: &crate::tui::state::EditorState<'_>,
    term_size: Rect,
) -> Vec<ScrollBlockRegion> {
    let mut blocks = vec![ScrollBlockRegion {
        id: ConsoleScrollBlock::EditorTabContent,
        rect: editor.content_area(term_size),
        content_w: editor.tab_content_width,
        content_h: editor.tab_content_height,
    }];
    if editor.active_tab == crate::tui::state::EditorTab::Mounts {
        let mounts = super::editor_scroll_area(editor, term_size);
        blocks.push(ScrollBlockRegion {
            id: ConsoleScrollBlock::EditorWorkspaceMounts,
            rect: mounts.area,
            content_w: mounts.content_width,
            content_h: editor.tab_content_height,
        });
    }
    blocks
}

fn settings_blocks(
    settings: &crate::tui::state::SettingsState<'_>,
    term_size: Rect,
) -> Vec<ScrollBlockRegion> {
    let area = settings.content_area(term_size);
    let tab = settings.active_tab;
    let block = match tab {
        crate::tui::state::SettingsTab::General => return Vec::new(),
        crate::tui::state::SettingsTab::Mounts => ScrollBlockRegion {
            id: ConsoleScrollBlock::SettingsMounts,
            rect: area,
            content_w: settings.mounts.content_width(),
            content_h: settings.mounts_content_height(),
        },
        crate::tui::state::SettingsTab::Environments => ScrollBlockRegion {
            id: ConsoleScrollBlock::SettingsEnv,
            rect: area,
            content_w: 0,
            content_h: settings.env_content_height(),
        },
        crate::tui::state::SettingsTab::Trust => ScrollBlockRegion {
            id: ConsoleScrollBlock::SettingsTrust,
            rect: area,
            content_w: crate::tui::screens::settings::update::trust_content_width(&settings.trust),
            content_h: settings.trust_content_height(),
        },
        crate::tui::state::SettingsTab::Auth => ScrollBlockRegion {
            id: ConsoleScrollBlock::SettingsAuth,
            rect: area,
            content_w: 0,
            content_h: settings.auth_content_height(),
        },
    };
    vec![block]
}
