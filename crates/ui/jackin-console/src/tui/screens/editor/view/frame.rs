// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod footer;
mod geometry;
mod lines;
mod screen;
mod tabs;
pub(crate) use footer::editor_contextual_footer_items;
pub(crate) use geometry::{prepare_editor_for_render, render_editor_with_footer};
pub(crate) use lines::{
    editor_auth_lines_for_state, editor_general_lines_for_state, editor_mount_lines_for_state,
    editor_role_lines_for_state, editor_secret_lines_for_state,
};
pub(crate) use screen::{editor_frame_areas, render_editor_screen, resolve_editor_frame};
pub(crate) use tabs::{
    editor_tab_content_focused, render_auth_tab, render_general_tab, render_mounts_tab,
    render_roles_tab, render_secrets_tab,
};
