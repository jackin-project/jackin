// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings selection and scroll plans.

use super::{step_cursor_down_by, step_cursor_up_by};

use super::super::model::{SettingsEnvRow, SettingsHoverTarget, SettingsTab, SettingsTrustState};

use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsSelectionScrollPlan {
    pub selected: usize,
    pub scroll_y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsTrustRowSelectPlan {
    pub selected: Option<usize>,
    pub content_focused: bool,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Four orthogonal settings-tab focus flags (mounts, env, auth, trust) \
              describing which sub-scroll pane is currently focusable — each tracks \
              a distinct sub-pane and is consumed individually by the focus router. \
              Mutually exclusive in practice but naming each sub-pane directly is \
              clearer than a single enum variant in plan-shaped code."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsScrollFocusPlan {
    pub mounts: bool,
    pub env: bool,
    pub auth: bool,
    pub trust: bool,
}

#[must_use]
pub fn settings_horizontal_scroll_plan(
    current_scroll_x: u16,
    delta: i16,
    term_width: u16,
    content_width: usize,
) -> u16 {
    crate::tui::update::term_width_scroll_plan(current_scroll_x, delta, term_width, content_width)
}

#[must_use]
pub const fn settings_scroll_focus_plan(
    active_tab: SettingsTab,
    modal_open: bool,
    in_content: bool,
) -> SettingsScrollFocusPlan {
    if modal_open {
        return SettingsScrollFocusPlan {
            mounts: false,
            env: false,
            auth: false,
            trust: false,
        };
    }
    SettingsScrollFocusPlan {
        mounts: matches!(active_tab, SettingsTab::Mounts) && in_content,
        env: matches!(active_tab, SettingsTab::Environments) && in_content,
        auth: matches!(active_tab, SettingsTab::Auth) && in_content,
        trust: matches!(active_tab, SettingsTab::Trust) && in_content,
    }
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Four orthogonal settings-modal visibility flags (error_popup, \
              mounts_modal, env_modal, auth_modal) — each is an independent \
              picker-open signal the modal-state resolver inspects. Named-arg \
              reads match the per-picker visibility-routing idiom."
)]
#[must_use]
pub const fn settings_modal_open(
    error_popup_open: bool,
    mounts_modal_open: bool,
    env_modal_open: bool,
    auth_modal_open: bool,
) -> bool {
    error_popup_open || mounts_modal_open || env_modal_open || auth_modal_open
}

#[must_use]
pub const fn settings_trust_row_select_plan(
    selected: usize,
    row_count: usize,
) -> SettingsTrustRowSelectPlan {
    SettingsTrustRowSelectPlan {
        selected: if selected < row_count {
            Some(selected)
        } else {
            None
        },
        content_focused: true,
    }
}

#[must_use]
pub fn settings_trust_selection_plan(
    selected: usize,
    row_count: usize,
    delta: isize,
    current_scroll_y: u16,
    term_height: u16,
    footer_h: u16,
) -> SettingsSelectionScrollPlan {
    let selected = crate::tui::focus::collection_move_index(selected, row_count, delta);
    SettingsSelectionScrollPlan {
        selected,
        scroll_y: crate::tui::focus::cursor_scroll_for_panel(
            selected,
            current_scroll_y,
            term_height,
            footer_h,
        ),
    }
}

#[must_use]
pub fn settings_env_selection_plan(
    selected: usize,
    rows: &[SettingsEnvRow],
    delta: isize,
    current_scroll_y: u16,
    term_height: u16,
    footer_h: u16,
) -> SettingsSelectionScrollPlan {
    let max = rows.len().saturating_sub(1);
    let candidate = if delta.is_negative() {
        selected.saturating_sub(delta.unsigned_abs())
    } else {
        selected.saturating_add(delta.unsigned_abs()).min(max)
    };
    let selected = if delta.is_negative() {
        step_cursor_up_by(candidate, |idx| {
            matches!(rows.get(idx), Some(SettingsEnvRow::SectionSpacer))
        })
    } else {
        step_cursor_down_by(candidate, max, |idx| {
            matches!(rows.get(idx), Some(SettingsEnvRow::SectionSpacer))
        })
    };
    SettingsSelectionScrollPlan {
        selected,
        scroll_y: crate::tui::focus::cursor_scroll_for_panel(
            selected,
            current_scroll_y,
            term_height,
            footer_h,
        ),
    }
}

#[must_use]
pub fn settings_global_mounts_selection_plan(
    selected: usize,
    mount_count: usize,
    delta: isize,
    current_scroll_y: u16,
    term_height: u16,
    footer_h: u16,
) -> SettingsSelectionScrollPlan {
    let selected = if delta.is_negative() {
        selected.saturating_sub(delta.unsigned_abs())
    } else {
        selected
            .saturating_add(delta.unsigned_abs())
            .min(mount_count)
    };
    SettingsSelectionScrollPlan {
        selected,
        scroll_y: crate::tui::focus::cursor_scroll_for_panel(
            selected,
            current_scroll_y,
            term_height,
            footer_h,
        ),
    }
}

#[must_use]
pub fn settings_global_mounts_selected_index(selected: usize, mount_count: usize) -> usize {
    selected.min(mount_count)
}

#[must_use]
pub const fn settings_global_mounts_add_row_selected(selected: usize, mount_count: usize) -> bool {
    selected == mount_count
}

#[must_use]
pub fn settings_global_mounts_added_index(mount_count: usize) -> usize {
    mount_count.saturating_sub(1)
}

#[must_use]
pub fn settings_trust_row_at_position(
    area: Rect,
    col: u16,
    row: u16,
    scroll_y: u16,
    row_count: usize,
) -> Option<usize> {
    if !crate::tui::layout::point_in_rect(col, row, area) {
        return None;
    }
    let line = usize::from(row.saturating_sub(area.y + 1)) + usize::from(scroll_y);
    let row = line.checked_sub(1)?;
    (row < row_count).then_some(row)
}

#[must_use]
pub fn settings_trust_hover_target_at_position(
    active_tab: SettingsTab,
    mounts_modal_open: bool,
    area: Rect,
    col: u16,
    row: u16,
    scroll_y: u16,
    row_count: usize,
) -> Option<SettingsHoverTarget> {
    if active_tab != SettingsTab::Trust || mounts_modal_open {
        return None;
    }
    settings_trust_row_at_position(area, col, row, scroll_y, row_count)
        .map(SettingsHoverTarget::TrustRow)
}

#[must_use]
pub fn settings_trust_clickable_at_position(
    active_tab: SettingsTab,
    modal_open: bool,
    content_area: Rect,
    col: u16,
    row: u16,
) -> bool {
    active_tab == SettingsTab::Trust
        && !modal_open
        && crate::tui::layout::point_in_rect(col, row, content_area)
}

#[must_use]
pub fn trust_content_width(state: &SettingsTrustState) -> usize {
    state
        .pending
        .iter()
        .map(|row| 42 + termrock::text::display_cols(&row.git))
        .chain(["  Role                         Trust      Git".len()])
        .max()
        .unwrap_or(0)
}
