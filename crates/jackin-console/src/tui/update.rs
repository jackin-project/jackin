// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Top-level console TUI update helpers.

use crossterm::event::{KeyEvent, KeyModifiers, MouseEventKind};

use crate::tui::components::{
    agent_choice::{AgentChoice, AgentChoiceState},
    provider_picker::ProviderPickerState,
};
use crate::tui::sidebar_layout::{SidebarScrollAreas, focused_mount_scroll_area_still_scrollable};
use jackin_tui::runtime::UpdateResult;

pub type ConsoleUpdate<E> = UpdateResult<E>;

#[derive(Debug, Clone)]
pub enum StatusOverlayPlan {
    Open(jackin_tui::components::StatusPopupState),
    Dismiss,
}

pub trait StatusOverlayState {
    fn set_status_overlay(&mut self, overlay: Option<jackin_tui::components::StatusPopupState>);
}

pub fn apply_status_overlay_plan(state: &mut impl StatusOverlayState, plan: StatusOverlayPlan) {
    match plan {
        StatusOverlayPlan::Open(overlay) => state.set_status_overlay(Some(overlay)),
        StatusOverlayPlan::Dismiss => state.set_status_overlay(None),
    }
}

#[derive(Debug)]
pub enum ListModalPlan {
    ContainerInfo(jackin_tui::components::ContainerInfoState),
    ErrorPopup(jackin_tui::components::ErrorPopupState),
    GithubPicker(crate::tui::components::github_picker::GithubPickerState),
    Dismiss,
}

pub trait ListModalState {
    fn open_container_info_modal(&mut self, state: jackin_tui::components::ContainerInfoState);
    fn open_error_popup_modal(&mut self, state: jackin_tui::components::ErrorPopupState);
    fn open_github_picker_modal(
        &mut self,
        state: crate::tui::components::github_picker::GithubPickerState,
    );
    fn dismiss_list_modal(&mut self);
}

pub fn apply_list_modal_plan(state: &mut impl ListModalState, plan: ListModalPlan) {
    match plan {
        ListModalPlan::ContainerInfo(info) => state.open_container_info_modal(info),
        ListModalPlan::ErrorPopup(error) => state.open_error_popup_modal(error),
        ListModalPlan::GithubPicker(picker) => state.open_github_picker_modal(picker),
        ListModalPlan::Dismiss => state.dismiss_list_modal(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlinePickerDismissal {
    NewSession,
    Role,
    Agent,
    Provider,
    LaunchProvider,
}

pub trait InlinePickerDismissalState {
    fn clear_inline_new_session_picker(&mut self);
    fn clear_inline_role_picker(&mut self);
    fn clear_inline_agent_picker(&mut self);
    fn clear_inline_provider_picker(&mut self);
    fn clear_launch_provider_picker(&mut self);
}

pub fn apply_inline_picker_dismissal_plan(
    state: &mut impl InlinePickerDismissalState,
    plan: InlinePickerDismissal,
) {
    match plan {
        InlinePickerDismissal::NewSession => state.clear_inline_new_session_picker(),
        InlinePickerDismissal::Role => state.clear_inline_role_picker(),
        InlinePickerDismissal::Agent => state.clear_inline_agent_picker(),
        InlinePickerDismissal::Provider => state.clear_inline_provider_picker(),
        InlinePickerDismissal::LaunchProvider => state.clear_launch_provider_picker(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlinePickerShellPlan {
    ScrollHorizontal(i16),
    Exit,
    Delegate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlinePickerPlan<T> {
    Commit(T),
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileBrowserModalPlan<T> {
    ApplyFileBrowserOutcome(crate::tui::components::file_browser::FileBrowserOutcome<T>),
    ResolveGitUrl(std::path::PathBuf),
    OpenUrl(String),
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountDstChoicePlan {
    CommitSamePath,
    OpenEditInput,
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDiscardModalPlan {
    Save,
    Discard,
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmSaveModalPlan {
    Commit,
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolConfirmModalPlan {
    Confirm,
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateOpPickerPlan<S> {
    Commit(S),
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSourceFolderPickerPlan<T> {
    Commit(T),
    Close,
    KeepModal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopePickerPlan {
    AllAgents,
    SpecificAgent,
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourcePickerPlan {
    Plain,
    Op,
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListGithubPickerPlan {
    OpenUrl(String),
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListModalKeyTarget {
    GithubPicker,
    RolePicker,
    ErrorPopup,
    ContainerInfo,
    Dismiss,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListRolePickerPlan<R> {
    Launch(R),
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DismissibleModalPlan {
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListModalScrollTarget {
    GithubPicker,
    RolePicker,
    OpPicker,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedModalScrollTarget {
    WorkdirPick,
    RolePicker,
    OpPicker,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEnvModalScrollTarget {
    OpPicker,
    RolePicker,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAuthModalScrollTarget {
    OpPicker,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalMountModalScrollTarget {
    RolePicker,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleMouseWheelPlan {
    Horizontal {
        delta: i16,
        vertical_fallback: Option<i16>,
    },
    Vertical(i16),
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListPreRenderFocusPlan {
    pub list_scroll_focus: Option<crate::tui::focus::MountScrollFocus>,
    pub list_names_focused: bool,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "Four orthogonal scroll-reset flags (reset_workspace, reset_global, \
              reset_role_global, reset_roles) — each is an independent reset \
              channel the list-pre-render plan applies to the corresponding scroll \
              area. Named-field reads match the per-area reset gating."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListPreRenderScrollResetPlan {
    pub reset_workspace: bool,
    pub reset_global: bool,
    pub reset_role_global: bool,
    pub reset_roles: bool,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "Six orthogonal list pre-render state flags (list_names_focused, \
              preview_focused, sidebar_available, focused_block_scrollable, \
              role_global_available, roles_available) — each tracks an independent \
              UI-scroll-availability signal consumed individually by the focus and \
              scroll-reset plans. Named-field reads match the per-pane gating idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListPreRenderFacts {
    pub list_scroll_focus: Option<crate::tui::focus::MountScrollFocus>,
    pub list_names_focused: bool,
    pub preview_focused: bool,
    pub sidebar_available: bool,
    pub focused_block_scrollable: bool,
    pub role_global_available: bool,
    pub roles_available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListPreRenderPlan {
    pub scroll_reset: ListPreRenderScrollResetPlan,
    pub focus: ListPreRenderFocusPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineProviderFollowupPlan<C, A, P> {
    StartSession { context: C, agent: A },
    OpenProviderPicker(ProviderPickerState<C, A, P>),
}

pub trait InlineNewSessionPickerState<C, A: AgentChoice, P> {
    fn set_inline_new_session_picker(
        &mut self,
        context: C,
        picker: AgentChoiceState<A>,
        providers: Vec<P>,
    );
}

pub fn apply_inline_new_session_picker_plan<C, A: AgentChoice, P>(
    state: &mut impl InlineNewSessionPickerState<C, A, P>,
    context: C,
    picker: AgentChoiceState<A>,
    providers: Vec<P>,
) {
    state.set_inline_new_session_picker(context, picker, providers);
}

pub trait InlineProviderPickerState<C, A, P> {
    fn set_inline_provider_picker(&mut self, picker: ProviderPickerState<C, A, P>);
}

pub fn apply_inline_provider_picker_plan<C, A, P>(
    state: &mut impl InlineProviderPickerState<C, A, P>,
    picker: ProviderPickerState<C, A, P>,
) {
    state.set_inline_provider_picker(picker);
}

#[must_use]
pub const fn list_scroll_focus_plan(
    focus: Option<crate::tui::focus::MountScrollFocus>,
) -> Option<crate::tui::focus::MountScrollFocus> {
    focus
}

#[must_use]
pub const fn list_names_focus_plan(focused: bool) -> bool {
    focused
}

#[allow(
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

#[allow(
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
) -> SettingsEnvModalScrollTarget {
    if op_picker {
        SettingsEnvModalScrollTarget::OpPicker
    } else if role_picker {
        SettingsEnvModalScrollTarget::RolePicker
    } else {
        SettingsEnvModalScrollTarget::None
    }
}

#[must_use]
pub const fn settings_auth_modal_scroll_target(op_picker: bool) -> SettingsAuthModalScrollTarget {
    if op_picker {
        SettingsAuthModalScrollTarget::OpPicker
    } else {
        SettingsAuthModalScrollTarget::None
    }
}

#[must_use]
pub const fn global_mount_modal_scroll_target(role_picker: bool) -> GlobalMountModalScrollTarget {
    if role_picker {
        GlobalMountModalScrollTarget::RolePicker
    } else {
        GlobalMountModalScrollTarget::None
    }
}

#[must_use]
pub fn console_mouse_wheel_plan(
    kind: MouseEventKind,
    modifiers: KeyModifiers,
) -> ConsoleMouseWheelPlan {
    let axes = jackin_tui::scroll::ScrollAxes {
        vertical: true,
        horizontal: true,
    };
    let Some(delta) = jackin_tui::scroll::mouse_scroll_delta_with_step(
        kind,
        modifiers,
        axes,
        crate::tui::layout::MOUSE_HORIZONTAL_SCROLL_STEP,
    ) else {
        return ConsoleMouseWheelPlan::None;
    };

    match delta.axis {
        jackin_tui::scroll::ScrollAxis::Horizontal => ConsoleMouseWheelPlan::Horizontal {
            delta: delta.amount,
            vertical_fallback: jackin_tui::scroll::mouse_scroll_delta_with_step(
                kind,
                modifiers,
                jackin_tui::scroll::ScrollAxes {
                    vertical: true,
                    horizontal: false,
                },
                crate::tui::layout::MOUSE_HORIZONTAL_SCROLL_STEP,
            )
            .map(|fallback| fallback.amount),
        },
        jackin_tui::scroll::ScrollAxis::Vertical => ConsoleMouseWheelPlan::Vertical(delta.amount),
    }
}

#[allow(
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

#[must_use]
pub fn inline_provider_followup_plan<C, A, P>(
    context: C,
    agent: A,
    providers: Vec<P>,
) -> InlineProviderFollowupPlan<C, A, P> {
    // Open the picker only when the operator has a real choice. A list of 0
    // or 1 means the caller collapsed out the agent's native auth (or never
    // passed any) — dispatch directly instead of presenting a one-item modal.
    if providers.len() >= 2 {
        InlineProviderFollowupPlan::OpenProviderPicker(ProviderPickerState::new(
            context, agent, providers,
        ))
    } else {
        InlineProviderFollowupPlan::StartSession { context, agent }
    }
}

#[must_use]
pub fn inline_picker_shell_plan(key: KeyEvent, _exit_on_q: bool) -> InlinePickerShellPlan {
    use crate::tui::keymap::{INLINE_PICKER_SHELL_KEYMAP, InlinePickerShellAction};
    use jackin_tui::components::KeyChord;
    let chord = KeyChord::from(key);
    match INLINE_PICKER_SHELL_KEYMAP.dispatch(chord) {
        Some(InlinePickerShellAction::ScrollLeft) => InlinePickerShellPlan::ScrollHorizontal(-8),
        Some(InlinePickerShellAction::ScrollRight) => InlinePickerShellPlan::ScrollHorizontal(8),
        None => InlinePickerShellPlan::Delegate,
    }
}

#[must_use]
pub fn inline_picker_plan<T>(outcome: jackin_tui::ModalOutcome<T>) -> InlinePickerPlan<T> {
    match outcome {
        jackin_tui::ModalOutcome::Commit(value) => InlinePickerPlan::Commit(value),
        jackin_tui::ModalOutcome::Cancel => InlinePickerPlan::Dismiss,
        jackin_tui::ModalOutcome::Continue => InlinePickerPlan::Continue,
    }
}

#[must_use]
pub fn file_browser_modal_plan<T>(
    outcome: crate::tui::components::file_browser::FileBrowserOutcome<T>,
) -> FileBrowserModalPlan<T> {
    match outcome {
        crate::tui::components::file_browser::FileBrowserOutcome::Cancel => {
            FileBrowserModalPlan::Dismiss
        }
        crate::tui::components::file_browser::FileBrowserOutcome::ResolveGitUrl(path) => {
            FileBrowserModalPlan::ResolveGitUrl(path)
        }
        crate::tui::components::file_browser::FileBrowserOutcome::OpenGitUrl(url) => {
            FileBrowserModalPlan::OpenUrl(url)
        }
        crate::tui::components::file_browser::FileBrowserOutcome::Continue => {
            FileBrowserModalPlan::Continue
        }
        crate::tui::components::file_browser::FileBrowserOutcome::Commit(_)
        | crate::tui::components::file_browser::FileBrowserOutcome::NavigateTo(_)
        | crate::tui::components::file_browser::FileBrowserOutcome::NavigateUp
        | crate::tui::components::file_browser::FileBrowserOutcome::RequestCommit(_) => {
            FileBrowserModalPlan::ApplyFileBrowserOutcome(outcome)
        }
    }
}

#[must_use]
pub fn auth_source_folder_picker_plan<T>(
    outcome: crate::tui::components::file_browser::FileBrowserOutcome<T>,
) -> AuthSourceFolderPickerPlan<T> {
    match outcome {
        crate::tui::components::file_browser::FileBrowserOutcome::Commit(path) => {
            AuthSourceFolderPickerPlan::Commit(path)
        }
        crate::tui::components::file_browser::FileBrowserOutcome::Cancel => {
            AuthSourceFolderPickerPlan::Close
        }
        crate::tui::components::file_browser::FileBrowserOutcome::Continue
        | crate::tui::components::file_browser::FileBrowserOutcome::OpenGitUrl(_)
        | crate::tui::components::file_browser::FileBrowserOutcome::ResolveGitUrl(_)
        | crate::tui::components::file_browser::FileBrowserOutcome::NavigateTo(_)
        | crate::tui::components::file_browser::FileBrowserOutcome::NavigateUp
        | crate::tui::components::file_browser::FileBrowserOutcome::RequestCommit(_) => {
            AuthSourceFolderPickerPlan::KeepModal
        }
    }
}

#[must_use]
pub const fn mount_dst_choice_plan(
    outcome: jackin_tui::ModalOutcome<crate::tui::components::mount_dst_choice::MountDstChoice>,
) -> MountDstChoicePlan {
    match outcome {
        jackin_tui::ModalOutcome::Commit(
            crate::tui::components::mount_dst_choice::MountDstChoice::SamePath,
        ) => MountDstChoicePlan::CommitSamePath,
        jackin_tui::ModalOutcome::Commit(
            crate::tui::components::mount_dst_choice::MountDstChoice::Edit,
        ) => MountDstChoicePlan::OpenEditInput,
        jackin_tui::ModalOutcome::Cancel => MountDstChoicePlan::Dismiss,
        jackin_tui::ModalOutcome::Continue => MountDstChoicePlan::Continue,
    }
}

#[must_use]
pub const fn save_discard_modal_plan(
    outcome: jackin_tui::ModalOutcome<jackin_tui::components::SaveDiscardChoice>,
) -> SaveDiscardModalPlan {
    match outcome {
        jackin_tui::ModalOutcome::Commit(jackin_tui::components::SaveDiscardChoice::Save) => {
            SaveDiscardModalPlan::Save
        }
        jackin_tui::ModalOutcome::Commit(jackin_tui::components::SaveDiscardChoice::Discard) => {
            SaveDiscardModalPlan::Discard
        }
        jackin_tui::ModalOutcome::Cancel => SaveDiscardModalPlan::Dismiss,
        jackin_tui::ModalOutcome::Continue => SaveDiscardModalPlan::Continue,
    }
}

#[must_use]
pub const fn confirm_save_modal_plan(
    outcome: jackin_tui::ModalOutcome<crate::tui::components::confirm_save::SaveChoice>,
) -> ConfirmSaveModalPlan {
    match outcome {
        jackin_tui::ModalOutcome::Commit(
            crate::tui::components::confirm_save::SaveChoice::Save,
        ) => ConfirmSaveModalPlan::Commit,
        jackin_tui::ModalOutcome::Cancel => ConfirmSaveModalPlan::Dismiss,
        jackin_tui::ModalOutcome::Continue => ConfirmSaveModalPlan::Continue,
    }
}

#[must_use]
pub const fn bool_confirm_modal_plan(
    outcome: jackin_tui::ModalOutcome<bool>,
) -> BoolConfirmModalPlan {
    match outcome {
        jackin_tui::ModalOutcome::Commit(true) => BoolConfirmModalPlan::Confirm,
        jackin_tui::ModalOutcome::Commit(false) | jackin_tui::ModalOutcome::Cancel => {
            BoolConfirmModalPlan::Dismiss
        }
        jackin_tui::ModalOutcome::Continue => BoolConfirmModalPlan::Continue,
    }
}

#[must_use]
pub fn create_op_picker_plan<Reference, Account, Vault, Item, FieldTarget>(
    outcome: jackin_tui::ModalOutcome<
        crate::tui::components::op_picker::OpPickerSelection<
            Reference,
            Account,
            Vault,
            Item,
            FieldTarget,
        >,
    >,
) -> CreateOpPickerPlan<
    crate::tui::components::op_picker::OpPickerSelection<
        Reference,
        Account,
        Vault,
        Item,
        FieldTarget,
    >,
> {
    match outcome {
        jackin_tui::ModalOutcome::Commit(selection) => match selection {
            crate::tui::components::op_picker::OpPickerSelection::NewItem { .. }
            | crate::tui::components::op_picker::OpPickerSelection::EditItemField { .. } => {
                CreateOpPickerPlan::Commit(selection)
            }
            crate::tui::components::op_picker::OpPickerSelection::Existing(_) => {
                CreateOpPickerPlan::Dismiss
            }
        },
        jackin_tui::ModalOutcome::Cancel => CreateOpPickerPlan::Dismiss,
        jackin_tui::ModalOutcome::Continue => CreateOpPickerPlan::Continue,
    }
}

#[must_use]
pub const fn scope_picker_plan(
    outcome: jackin_tui::ModalOutcome<crate::tui::components::scope_picker::ScopeChoice>,
) -> ScopePickerPlan {
    match outcome {
        jackin_tui::ModalOutcome::Commit(
            crate::tui::components::scope_picker::ScopeChoice::AllAgents,
        ) => ScopePickerPlan::AllAgents,
        jackin_tui::ModalOutcome::Commit(
            crate::tui::components::scope_picker::ScopeChoice::SpecificAgent,
        ) => ScopePickerPlan::SpecificAgent,
        jackin_tui::ModalOutcome::Cancel => ScopePickerPlan::Dismiss,
        jackin_tui::ModalOutcome::Continue => ScopePickerPlan::Continue,
    }
}

#[must_use]
pub const fn source_picker_plan(
    outcome: jackin_tui::ModalOutcome<crate::tui::components::source_picker::SourceChoice>,
) -> SourcePickerPlan {
    match outcome {
        jackin_tui::ModalOutcome::Commit(
            crate::tui::components::source_picker::SourceChoice::Plain,
        ) => SourcePickerPlan::Plain,
        jackin_tui::ModalOutcome::Commit(
            crate::tui::components::source_picker::SourceChoice::Op,
        ) => SourcePickerPlan::Op,
        jackin_tui::ModalOutcome::Cancel => SourcePickerPlan::Dismiss,
        jackin_tui::ModalOutcome::Continue => SourcePickerPlan::Continue,
    }
}

#[must_use]
pub fn list_github_picker_plan(outcome: jackin_tui::ModalOutcome<String>) -> ListGithubPickerPlan {
    match outcome {
        jackin_tui::ModalOutcome::Commit(url) => ListGithubPickerPlan::OpenUrl(url),
        jackin_tui::ModalOutcome::Cancel => ListGithubPickerPlan::Dismiss,
        jackin_tui::ModalOutcome::Continue => ListGithubPickerPlan::Continue,
    }
}

#[must_use]
pub fn list_role_picker_plan<R>(outcome: jackin_tui::ModalOutcome<R>) -> ListRolePickerPlan<R> {
    match outcome {
        jackin_tui::ModalOutcome::Commit(role) => ListRolePickerPlan::Launch(role),
        jackin_tui::ModalOutcome::Cancel => ListRolePickerPlan::Dismiss,
        jackin_tui::ModalOutcome::Continue => ListRolePickerPlan::Continue,
    }
}

#[must_use]
pub fn dismissible_modal_plan<T>(outcome: jackin_tui::ModalOutcome<T>) -> DismissibleModalPlan {
    match outcome {
        jackin_tui::ModalOutcome::Commit(_) | jackin_tui::ModalOutcome::Cancel => {
            DismissibleModalPlan::Dismiss
        }
        jackin_tui::ModalOutcome::Continue => DismissibleModalPlan::Continue,
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

pub trait ListShellState {
    fn set_drag_state(&mut self, drag: Option<crate::tui::split::DragState>);
    fn set_list_split_pct(&mut self, pct: u16);
}

pub fn apply_drag_state_plan(
    state: &mut impl ListShellState,
    plan: Option<crate::tui::split::DragState>,
) {
    state.set_drag_state(plan);
}

pub fn apply_list_split_pct_plan(state: &mut impl ListShellState, plan: u16) {
    state.set_list_split_pct(plan);
}

#[must_use]
pub fn selection_move_plan(selected: usize, row_count: usize, delta: isize) -> usize {
    crate::tui::focus::moved_selection(selected, row_count, delta)
}

#[must_use]
pub fn selected_index_plan(selected: usize, row_count: usize) -> usize {
    crate::tui::focus::selected_index(selected, row_count)
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
pub fn role_resolution_status_overlay_plan(role_key: impl std::fmt::Display) -> StatusOverlayPlan {
    StatusOverlayPlan::Open(
        crate::tui::components::status_popup::role_resolution_status_popup_state(role_key),
    )
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
pub fn open_error_popup_modal_plan(
    title: impl Into<String>,
    message: impl Into<String>,
) -> ListModalPlan {
    ListModalPlan::ErrorPopup(crate::tui::components::error_popup::error_popup_state(
        title, message,
    ))
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
mod tests;
