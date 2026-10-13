// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `view`.

use super::*;

#[cfg(any(test, feature = "test-support"))]
use jackin_config::fixtures::config_with_agents;

use std::collections::BTreeMap;

use super::prepare_editor_tab_for_area;

use super::render_roles_tab;

use crate::tui::state::{EditorState, EditorTab, FieldFocus};

use jackin_config::AppConfig;

use jackin_config::WorkspaceConfig;

use ratatui::Terminal;

use ratatui::backend::TestBackend;

use ratatui::layout::Rect;

use termrock::scroll::viewport_width as scroll_viewport_width;

use jackin_config::MountConfig;

use termrock::widgets::HintSpan;

use crate::tui::screens::editor::view::editor_contextual_footer_items as contextual_row_items;

use super::render_general_tab;

use super::render_editor_with_footer as render_editor;

use super::render_secrets_tab;

use jackin_config::WorkspaceRoleOverride;

mod frame_regression;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
