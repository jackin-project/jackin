// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Clickability and mouse-layer plans.

use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleClickStageFacts {
    List {
        list_modal_open: bool,
        workspace_list_target: bool,
    },
    Editor {
        modal_open: bool,
        tab_target: bool,
        mount_row_target: bool,
        auth_row_target: bool,
    },
    Settings {
        mounts_modal_open: bool,
        env_modal_open: bool,
        tab_target: bool,
        trust_target: bool,
    },
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsoleClickabilityFacts {
    pub pointer_supported: bool,
    pub file_browser_url_target: bool,
    pub container_info_copy_target: bool,
    pub stage: ConsoleClickStageFacts,
}

// Mouse parity matrix row 16 carve-out: the pointer-shape cue stays
// consumer code — upstream hit regions carry geometry only, no
// clickability classification. Facts derive from the same hit geometry
// the dispatch path uses.
#[must_use]
pub const fn console_clickable_at(facts: ConsoleClickabilityFacts) -> bool {
    if !facts.pointer_supported {
        return false;
    }
    if facts.file_browser_url_target || facts.container_info_copy_target {
        return true;
    }
    match facts.stage {
        ConsoleClickStageFacts::List {
            list_modal_open,
            workspace_list_target,
        } => !list_modal_open && workspace_list_target,
        ConsoleClickStageFacts::Editor {
            modal_open,
            tab_target,
            mount_row_target,
            auth_row_target,
        } => !modal_open && (tab_target || mount_row_target || auth_row_target),
        ConsoleClickStageFacts::Settings {
            mounts_modal_open,
            env_modal_open,
            tab_target,
            trust_target,
        } => !mounts_modal_open && !env_modal_open && (tab_target || trust_target),
        ConsoleClickStageFacts::Other => false,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConsoleModalMouseFacts {
    pub quit_confirm_open: bool,
    pub list_modal_open: bool,
    pub list_modal_container_info: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConsoleModalMouseLayerFacts {
    pub quit_confirm_rect: Option<Rect>,
    pub list_modal_rect: Option<Rect>,
    pub list_modal_container_info: bool,
    pub startup_error_modal_active: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConsoleModalMouseLayerPlan {
    pub consumed: bool,
    pub dismiss_quit_confirm: bool,
    pub dismiss_list_modal: bool,
}

#[must_use]
pub fn modal_mouse_layer_plan(
    mouse: crossterm::event::MouseEvent,
    facts: ConsoleModalMouseLayerFacts,
) -> ConsoleModalMouseLayerPlan {
    if let Some(rect) = facts.quit_confirm_rect {
        return ConsoleModalMouseLayerPlan {
            consumed: true,
            dismiss_quit_confirm: mouse_down_outside_rect(mouse, rect),
            dismiss_list_modal: false,
        };
    }

    let Some(rect) = facts.list_modal_rect else {
        return ConsoleModalMouseLayerPlan::default();
    };

    let consumed = modal_mouse_layer_consumes(
        mouse,
        ConsoleModalMouseFacts {
            quit_confirm_open: false,
            list_modal_open: true,
            list_modal_container_info: facts.list_modal_container_info,
        },
    );

    ConsoleModalMouseLayerPlan {
        consumed,
        dismiss_quit_confirm: false,
        dismiss_list_modal: !facts.startup_error_modal_active
            && mouse_down_outside_rect(mouse, rect),
    }
}

#[must_use]
pub const fn modal_mouse_layer_consumes(
    mouse: crossterm::event::MouseEvent,
    facts: ConsoleModalMouseFacts,
) -> bool {
    if facts.quit_confirm_open {
        return true;
    }
    if facts.list_modal_open {
        return !(mouse_is_wheel(mouse) && facts.list_modal_container_info);
    }
    false
}

#[must_use]
pub const fn debug_chip_activation_allowed(
    mouse: crossterm::event::MouseEvent,
    no_modal_open: bool,
    debug_chip_hovered: bool,
    active_run_present: bool,
) -> bool {
    matches!(mouse.kind, crossterm::event::MouseEventKind::Down(_))
        && no_modal_open
        && debug_chip_hovered
        && active_run_present
}

#[must_use]
pub const fn console_pointer_shape(
    chrome_hovered: bool,
    base_clickable: bool,
) -> termrock::osc::PointerShape {
    if chrome_hovered || base_clickable {
        termrock::osc::PointerShape::Pointer
    } else {
        termrock::osc::PointerShape::Default
    }
}

pub(crate) const fn mouse_is_wheel(mouse: crossterm::event::MouseEvent) -> bool {
    matches!(
        mouse.kind,
        crossterm::event::MouseEventKind::ScrollUp
            | crossterm::event::MouseEventKind::ScrollDown
            | crossterm::event::MouseEventKind::ScrollLeft
            | crossterm::event::MouseEventKind::ScrollRight
    )
}

pub(crate) fn mouse_down_outside_rect(mouse: crossterm::event::MouseEvent, rect: Rect) -> bool {
    matches!(mouse.kind, crossterm::event::MouseEventKind::Down(_))
        && !rect.contains(ratatui::layout::Position {
            x: mouse.column,
            y: mouse.row,
        })
}

#[must_use]
pub fn should_dismiss_list_modal_for_outside_click(
    startup_error_modal_active: bool,
    modal_rect: Rect,
    column: u16,
    row: u16,
) -> bool {
    if startup_error_modal_active {
        return false;
    }

    !modal_rect.contains(ratatui::layout::Position { x: column, y: row })
}
