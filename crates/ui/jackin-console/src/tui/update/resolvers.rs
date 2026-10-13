// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Scroll, key, and pre-render resolvers.

use super::{
    ConsoleMouseWheelPlan, ListModalKeyTarget, ListModalScrollTarget, ListPreRenderFacts,
    ListPreRenderFocusPlan, ListPreRenderPlan, ListPreRenderScrollResetPlan,
    SettingsModalScrollTarget, SharedModalScrollTarget,
};
use crossterm::event::{KeyModifiers, MouseEventKind};

use crate::tui::sidebar_layout::{SidebarScrollAreas, focused_mount_scroll_area_still_scrollable};

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Four mutually-exclusive modal visibility flags (github_picker, \
              role_picker, error_popup, container_info) — each is an independent \
              picker-open signal routed in priority order by the key-target \
              resolver. Named-arg reads match the per-picker key-routing idiom."
)]
#[must_use]
pub const fn list_modal_key_target(
    github_picker: bool,
    role_picker: bool,
    error_popup: bool,
    container_info: bool,
) -> ListModalKeyTarget {
    if github_picker {
        ListModalKeyTarget::GithubPicker
    } else if role_picker {
        ListModalKeyTarget::RolePicker
    } else if error_popup {
        ListModalKeyTarget::ErrorPopup
    } else if container_info {
        ListModalKeyTarget::ContainerInfo
    } else {
        ListModalKeyTarget::Dismiss
    }
}

#[must_use]
pub const fn list_modal_scroll_target(
    github_picker: bool,
    role_picker: bool,
    op_picker: bool,
) -> ListModalScrollTarget {
    if github_picker {
        ListModalScrollTarget::GithubPicker
    } else if role_picker {
        ListModalScrollTarget::RolePicker
    } else if op_picker {
        ListModalScrollTarget::OpPicker
    } else {
        ListModalScrollTarget::None
    }
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Five orthogonal modal visibility flags (workdir_pick, \
              role_picker, op_picker, settings pickers) — each is an independent \
              scroll target signal routed by the shared-modal scroll resolver. \
              Named-arg reads match the per-modal scroll-routing idiom."
)]
#[must_use]
pub const fn shared_modal_scroll_target(
    workdir_pick: bool,
    role_picker: bool,
    role_override_picker: bool,
    auth_role_picker: bool,
    op_picker: bool,
) -> SharedModalScrollTarget {
    if workdir_pick {
        SharedModalScrollTarget::WorkdirPick
    } else if role_picker || role_override_picker || auth_role_picker {
        SharedModalScrollTarget::RolePicker
    } else if op_picker {
        SharedModalScrollTarget::OpPicker
    } else {
        SharedModalScrollTarget::None
    }
}

#[must_use]
pub const fn settings_env_modal_scroll_target(
    op_picker: bool,
    role_picker: bool,
) -> SettingsModalScrollTarget {
    if op_picker {
        SettingsModalScrollTarget::EnvOpPicker
    } else if role_picker {
        SettingsModalScrollTarget::EnvRolePicker
    } else {
        SettingsModalScrollTarget::None
    }
}

#[must_use]
pub const fn settings_auth_modal_scroll_target(op_picker: bool) -> SettingsModalScrollTarget {
    if op_picker {
        SettingsModalScrollTarget::AuthOpPicker
    } else {
        SettingsModalScrollTarget::None
    }
}

#[must_use]
pub const fn global_mount_modal_scroll_target(role_picker: bool) -> SettingsModalScrollTarget {
    if role_picker {
        SettingsModalScrollTarget::MountRolePicker
    } else {
        SettingsModalScrollTarget::None
    }
}

#[must_use]
pub fn console_mouse_wheel_plan(
    kind: MouseEventKind,
    modifiers: KeyModifiers,
) -> ConsoleMouseWheelPlan {
    let axes = termrock::scroll::ScrollAxes {
        vertical: true,
        horizontal: true,
    };
    let Some(delta) = termrock::scroll::mouse_scroll_delta_with_step(
        kind.into(),
        modifiers.into(),
        axes,
        crate::tui::layout::MOUSE_HORIZONTAL_SCROLL_STEP,
    ) else {
        return ConsoleMouseWheelPlan::None;
    };

    match delta.axis {
        termrock::scroll::ScrollAxis::Horizontal => ConsoleMouseWheelPlan::Horizontal {
            delta: delta.amount,
            vertical_fallback: termrock::scroll::mouse_scroll_delta_with_step(
                kind.into(),
                modifiers.into(),
                termrock::scroll::ScrollAxes {
                    vertical: true,
                    horizontal: false,
                },
                crate::tui::layout::MOUSE_HORIZONTAL_SCROLL_STEP,
            )
            .map(|fallback| fallback.amount),
        },
        termrock::scroll::ScrollAxis::Vertical => ConsoleMouseWheelPlan::Vertical(delta.amount),
    }
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Four orthogonal focus decision inputs (list_names_focused, \
              preview_focused, sidebar_available, focused_block_scrollable) — each \
              is an independent UI signal the focus planner reads individually. \
              Named-arg reads match the per-branch focus routing idiom."
)]
#[must_use]
pub const fn list_pre_render_focus_plan(
    list_scroll_focus: Option<crate::tui::focus::MountScrollFocus>,
    list_names_focused: bool,
    preview_focused: bool,
    sidebar_available: bool,
    focused_block_scrollable: bool,
) -> ListPreRenderFocusPlan {
    if !sidebar_available {
        return ListPreRenderFocusPlan {
            list_scroll_focus: None,
            list_names_focused: if preview_focused {
                list_names_focused
            } else {
                true
            },
        };
    }

    if list_scroll_focus.is_some() && !focused_block_scrollable {
        return ListPreRenderFocusPlan {
            list_scroll_focus: None,
            list_names_focused: true,
        };
    }

    ListPreRenderFocusPlan {
        list_scroll_focus,
        list_names_focused,
    }
}

#[must_use]
pub const fn list_pre_render_scroll_reset_plan(
    sidebar_available: bool,
    role_global_available: bool,
    roles_available: bool,
) -> ListPreRenderScrollResetPlan {
    if !sidebar_available {
        return ListPreRenderScrollResetPlan {
            reset_workspace: true,
            reset_global: true,
            reset_role_global: true,
            reset_roles: true,
        };
    }

    ListPreRenderScrollResetPlan {
        reset_workspace: false,
        reset_global: false,
        reset_role_global: !role_global_available,
        reset_roles: !roles_available,
    }
}

#[must_use]
pub const fn list_pre_render_plan(facts: ListPreRenderFacts) -> ListPreRenderPlan {
    ListPreRenderPlan {
        scroll_reset: list_pre_render_scroll_reset_plan(
            facts.sidebar_available,
            facts.role_global_available,
            facts.roles_available,
        ),
        focus: list_pre_render_focus_plan(
            facts.list_scroll_focus,
            facts.list_names_focused,
            facts.preview_focused,
            facts.sidebar_available,
            facts.focused_block_scrollable,
        ),
    }
}

#[must_use]
pub fn list_pre_render_facts_from_scroll_areas(
    list_scroll_focus: Option<crate::tui::focus::MountScrollFocus>,
    list_names_focused: bool,
    preview_focused: bool,
    sidebar_areas: Option<&SidebarScrollAreas>,
) -> ListPreRenderFacts {
    ListPreRenderFacts {
        list_scroll_focus,
        list_names_focused,
        preview_focused,
        sidebar_available: sidebar_areas.is_some(),
        focused_block_scrollable: list_scroll_focus
            .is_none_or(|focus| focused_mount_scroll_area_still_scrollable(focus, sidebar_areas)),
        role_global_available: sidebar_areas.and_then(|areas| areas.role_global).is_some(),
        roles_available: sidebar_areas.and_then(|areas| areas.roles).is_some(),
    }
}
