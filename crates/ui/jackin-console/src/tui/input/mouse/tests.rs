// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Mouse drag-resize tests for the console TUI.
//! Unit tests for `handle_mouse`: the list/details seam is a
//! mouse-draggable resize affordance driven entirely from `ManagerState`.
//! These build `MouseEvent` values directly and bypass the ratatui
//! event loop — enough to pin the seam hit-test + drag math without a
//! real terminal.

use super::super::InputOutcome;
use super::{
    ConsoleHoverTarget, ConsoleScrollBlock, ScrollBlockRegion, container_info_copyable_row_at,
    file_browser_url_row_at, global_mounts_content_width, handle_mouse, handle_mouse_with_config,
    hit, list_scroll_areas, max_scroll_offset, workspace_mounts_content_width,
};

use crate::tui::components::save_discard::editor_exit_save_discard_state;

use crate::tui::layout::MOUSE_HORIZONTAL_SCROLL_STEP;

use crate::tui::screens::settings::view::global_mount_confirm_state;

use crate::tui::state::ManagerEffect;

use crate::tui::state::{
    DEFAULT_SPLIT_PCT, EditorHoverTarget, EditorState, EditorTab, FieldFocus, GlobalMountConfirm,
    MAX_SPLIT_PCT, MIN_SPLIT_PCT, ManagerHoverTarget, ManagerListRow, ManagerStage, ManagerState,
    Modal, MountScrollFocus, SecretsScopeTag, SettingsHoverTarget, SettingsModal, SettingsState,
    SettingsTab, SettingsTrustRow,
};

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};

use jackin_config::{MountConfig, WorkspaceConfig};

use ratatui::layout::Rect;

use super::{SCREEN_HEADER_HEIGHT, TAB_STRIP_HEIGHT};
mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
