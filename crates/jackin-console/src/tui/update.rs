//! Top-level console TUI update helpers.

use crate::tui::components::provider_picker::ProviderPickerState;
use jackin_tui::runtime::UpdateResult;

pub type ConsoleUpdate<E> = UpdateResult<E>;

#[derive(Debug, Clone)]
pub enum StatusOverlayPlan {
    Open(jackin_tui::components::StatusPopupState),
    Dismiss,
}

#[derive(Debug)]
pub enum ListModalPlan {
    ContainerInfo(jackin_tui::components::ContainerInfoState),
    GithubPicker(crate::tui::components::github_picker::GithubPickerState),
    Dismiss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlinePickerDismissal {
    NewSession,
    Role,
    Agent,
    Provider,
    LaunchProvider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListPreRenderFocusPlan {
    pub list_scroll_focus: Option<crate::focus::MountScrollFocus>,
    pub list_names_focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListPreRenderScrollResetPlan {
    pub reset_workspace: bool,
    pub reset_global: bool,
    pub reset_role_global: bool,
    pub reset_roles: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineProviderFollowupPlan<C, A, P> {
    StartSession {
        context: C,
        agent: A,
    },
    OpenProviderPicker(ProviderPickerState<C, A, P>),
}

#[must_use]
pub const fn list_scroll_focus_plan(
    focus: Option<crate::focus::MountScrollFocus>,
) -> Option<crate::focus::MountScrollFocus> {
    focus
}

#[must_use]
pub const fn list_names_focus_plan(focused: bool) -> bool {
    focused
}

#[must_use]
pub const fn list_pre_render_focus_plan(
    list_scroll_focus: Option<crate::focus::MountScrollFocus>,
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
pub fn inline_provider_followup_plan<C, A, P>(
    context: C,
    agent: A,
    providers: Vec<P>,
    agent_supports_providers: bool,
) -> InlineProviderFollowupPlan<C, A, P> {
    if agent_supports_providers && !providers.is_empty() {
        InlineProviderFollowupPlan::OpenProviderPicker(ProviderPickerState::new(
            context, agent, providers,
        ))
    } else {
        InlineProviderFollowupPlan::StartSession { context, agent }
    }
}

#[must_use]
pub const fn drag_state_plan(
    drag: Option<crate::tui::split::DragState>,
) -> Option<crate::tui::split::DragState> {
    drag
}

#[must_use]
pub const fn list_split_pct_plan(pct: u16) -> u16 {
    crate::tui::split::clamp_split(pct)
}

#[must_use]
pub fn selection_move_plan(selected: usize, row_count: usize, delta: isize) -> usize {
    crate::focus::moved_selection(selected, row_count, delta)
}

#[must_use]
pub fn selected_index_plan(selected: usize, row_count: usize) -> usize {
    crate::focus::selected_index(selected, row_count)
}

#[must_use]
pub const fn unclamped_scroll_plan(current_scroll: u16, delta: i16) -> u16 {
    let mut scroll = current_scroll;
    jackin_tui::components::apply_scroll_delta_unclamped(&mut scroll, delta);
    scroll
}

#[must_use]
pub fn term_width_scroll_plan(
    current_scroll_x: u16,
    delta: i16,
    term_width: u16,
    content_width: usize,
) -> u16 {
    let mut scroll_x = current_scroll_x;
    jackin_tui::components::apply_term_width_scroll_delta(
        &mut scroll_x,
        delta,
        term_width,
        content_width,
    );
    scroll_x
}

#[must_use]
pub fn open_status_overlay_plan(
    title: impl Into<String>,
    message: impl Into<String>,
) -> StatusOverlayPlan {
    StatusOverlayPlan::Open(crate::tui::components::status_popup::status_popup_state(
        title, message,
    ))
}

#[must_use]
pub const fn dismiss_status_overlay_plan() -> StatusOverlayPlan {
    StatusOverlayPlan::Dismiss
}

#[must_use]
pub fn open_container_info_modal_plan(
    state: jackin_tui::components::ContainerInfoState,
) -> ListModalPlan {
    ListModalPlan::ContainerInfo(state)
}

#[must_use]
pub fn open_github_picker_modal_plan(
    state: crate::tui::components::github_picker::GithubPickerState,
) -> ListModalPlan {
    ListModalPlan::GithubPicker(state)
}

#[must_use]
pub const fn dismiss_list_modal_plan() -> ListModalPlan {
    ListModalPlan::Dismiss
}

#[must_use]
pub const fn inline_picker_dismissal_plan(kind: InlinePickerDismissal) -> InlinePickerDismissal {
    kind
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_width_scroll_plan_updates_and_clamps_offset() {
        assert_eq!(term_width_scroll_plan(0, 8, 10, 40), 8);
        assert_eq!(term_width_scroll_plan(8, -99, 10, 40), 0);
    }

    #[test]
    fn selection_move_plan_clamps_to_rows() {
        assert_eq!(selection_move_plan(0, 3, 99), 2);
        assert_eq!(selection_move_plan(2, 3, -99), 0);
    }

    #[test]
    fn selected_index_plan_clamps_to_rows() {
        assert_eq!(selected_index_plan(99, 3), 2);
        assert_eq!(selected_index_plan(0, 0), 0);
    }

    #[test]
    fn unclamped_scroll_plan_updates_without_upper_clamp() {
        assert_eq!(unclamped_scroll_plan(4, 3), 7);
        assert_eq!(unclamped_scroll_plan(4, -99), 0);
    }

    #[test]
    fn status_overlay_plans_construct_open_and_dismiss() {
        let StatusOverlayPlan::Open(state) = open_status_overlay_plan("Title", "Body") else {
            panic!("expected open plan");
        };
        let debug = format!("{state:?}");
        assert!(debug.contains("Title"));
        assert!(debug.contains("Body"));
        assert!(matches!(
            dismiss_status_overlay_plan(),
            StatusOverlayPlan::Dismiss
        ));
    }

    #[test]
    fn inline_picker_dismissal_plan_returns_requested_kind() {
        assert_eq!(
            inline_picker_dismissal_plan(InlinePickerDismissal::Agent),
            InlinePickerDismissal::Agent
        );
    }

    #[test]
    fn shell_state_plans_return_normalized_values() {
        assert_eq!(
            list_scroll_focus_plan(Some(crate::focus::MountScrollFocus::Workspace)),
            Some(crate::focus::MountScrollFocus::Workspace)
        );
        assert!(list_names_focus_plan(true));
        let drag = crate::tui::split::DragState {
            anchor_pct: 30,
            anchor_x: 12,
        };
        assert_eq!(drag_state_plan(Some(drag)), Some(drag));
        assert_eq!(list_split_pct_plan(1), crate::tui::split::MIN_SPLIT_PCT);
        assert_eq!(list_split_pct_plan(99), crate::tui::split::MAX_SPLIT_PCT);
    }

    #[test]
    fn list_pre_render_focus_plan_handles_sidebar_liveness() {
        let missing_sidebar = list_pre_render_focus_plan(
            Some(crate::focus::MountScrollFocus::Workspace),
            false,
            false,
            false,
            false,
        );
        assert_eq!(missing_sidebar.list_scroll_focus, None);
        assert!(missing_sidebar.list_names_focused);

        let preview_missing_sidebar = list_pre_render_focus_plan(
            Some(crate::focus::MountScrollFocus::Workspace),
            false,
            true,
            false,
            false,
        );
        assert_eq!(preview_missing_sidebar.list_scroll_focus, None);
        assert!(!preview_missing_sidebar.list_names_focused);

        let stale_focus = list_pre_render_focus_plan(
            Some(crate::focus::MountScrollFocus::Workspace),
            false,
            true,
            true,
            false,
        );
        assert_eq!(stale_focus.list_scroll_focus, None);
        assert!(stale_focus.list_names_focused);

        let live_focus = list_pre_render_focus_plan(
            Some(crate::focus::MountScrollFocus::Workspace),
            false,
            false,
            true,
            true,
        );
        assert_eq!(
            live_focus.list_scroll_focus,
            Some(crate::focus::MountScrollFocus::Workspace)
        );
        assert!(!live_focus.list_names_focused);
    }

    #[test]
    fn list_pre_render_scroll_reset_plan_resets_missing_scroll_slots() {
        assert_eq!(
            list_pre_render_scroll_reset_plan(false, false, false),
            ListPreRenderScrollResetPlan {
                reset_workspace: true,
                reset_global: true,
                reset_role_global: true,
                reset_roles: true,
            }
        );
        assert_eq!(
            list_pre_render_scroll_reset_plan(true, false, true),
            ListPreRenderScrollResetPlan {
                reset_workspace: false,
                reset_global: false,
                reset_role_global: true,
                reset_roles: false,
            }
        );
        assert_eq!(
            list_pre_render_scroll_reset_plan(true, true, false),
            ListPreRenderScrollResetPlan {
                reset_workspace: false,
                reset_global: false,
                reset_role_global: false,
                reset_roles: true,
            }
        );
    }

    #[test]
    fn inline_provider_followup_plan_opens_picker_only_when_supported() {
        assert_eq!(
            inline_provider_followup_plan("container", "claude", vec!["zai"], true),
            InlineProviderFollowupPlan::OpenProviderPicker(
                ProviderPickerState::new("container", "claude", vec!["zai"])
            )
        );
        assert_eq!(
            inline_provider_followup_plan("container", "codex", vec!["zai"], false),
            InlineProviderFollowupPlan::StartSession {
                context: "container",
                agent: "codex",
            }
        );
        assert_eq!(
            inline_provider_followup_plan::<_, _, &str>("container", "claude", Vec::new(), true),
            InlineProviderFollowupPlan::StartSession {
                context: "container",
                agent: "claude",
            }
        );
    }
}
