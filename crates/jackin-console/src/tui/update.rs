//! Top-level console TUI update helpers.

use crossterm::event::{KeyCode, KeyEvent};

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
pub struct ListPreRenderFocusPlan {
    pub list_scroll_focus: Option<crate::tui::focus::MountScrollFocus>,
    pub list_names_focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListPreRenderScrollResetPlan {
    pub reset_workspace: bool,
    pub reset_global: bool,
    pub reset_role_global: bool,
    pub reset_roles: bool,
}

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
pub fn inline_picker_shell_plan(key: KeyEvent, exit_on_q: bool) -> InlinePickerShellPlan {
    match key.code {
        KeyCode::Left | KeyCode::Char('h' | 'H') => InlinePickerShellPlan::ScrollHorizontal(-8),
        KeyCode::Right | KeyCode::Char('l' | 'L') => InlinePickerShellPlan::ScrollHorizontal(8),
        KeyCode::Char('q' | 'Q') if exit_on_q => InlinePickerShellPlan::Exit,
        _ => InlinePickerShellPlan::Delegate,
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
mod tests;
