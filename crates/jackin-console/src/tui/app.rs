//! Top-level console TUI app model.

use std::path::PathBuf;

use ratatui::layout::Rect;

use crate::tui::components::footer_hints::{
    ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalContainerInfoFooterState,
    ModalFileBrowserFooterState, ModalFooterMode, ModalOpPickerFooterState,
};
use crate::tui::components::modal_rects::{
    ModalAuthFormState, ModalConfirmSavePrepareState, ModalConfirmSaveState, ModalConfirmState,
    ModalContainerInfoState, ModalErrorPopupState, ModalGithubPickerState, ModalOpPickerState,
    ModalRectMode, ModalRolePickerState,
};
use crate::tui::screens::editor::model::CreateStep;

/// Single-variant today; kept as `enum` so future stages can land without
/// churning every match site.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ConsoleAppStage<Manager> {
    Manager(Manager),
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ConsoleManagerStage<CreatePrelude, Editor, Settings> {
    List,
    Editor(Editor),
    Settings(Settings),
    CreatePrelude(CreatePrelude),
    ConfirmDelete {
        name: String,
        state: jackin_tui::components::ConfirmState,
    },
    ConfirmInstancePurge {
        container: String,
        label: String,
        state: jackin_tui::components::ConfirmState,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleManagerStageRoute {
    List,
    Editor,
    Settings,
    CreatePrelude,
    ConfirmDelete,
    ConfirmInstancePurge,
}

impl<CreatePrelude, Editor, Settings> ConsoleManagerStage<CreatePrelude, Editor, Settings> {
    #[must_use]
    pub const fn route(&self) -> ConsoleManagerStageRoute {
        match self {
            Self::List => ConsoleManagerStageRoute::List,
            Self::Editor(_) => ConsoleManagerStageRoute::Editor,
            Self::Settings(_) => ConsoleManagerStageRoute::Settings,
            Self::CreatePrelude(_) => ConsoleManagerStageRoute::CreatePrelude,
            Self::ConfirmDelete { .. } => ConsoleManagerStageRoute::ConfirmDelete,
            Self::ConfirmInstancePurge { .. } => ConsoleManagerStageRoute::ConfirmInstancePurge,
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ConsoleModal<
    TextInputTarget,
    TextInputState,
    FileBrowserTarget,
    FileBrowserState,
    MountDstChoiceState,
    WorkdirPickState,
    ConfirmTarget,
    ConfirmState,
    SaveDiscardState,
    GithubPickerState,
    ConfirmSaveState,
    ErrorPopupState,
    ContainerInfoState,
    StatusPopupState,
    OpPickerState,
    RolePickerState,
    SourcePickerState,
    ScopePickerState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
    SecretsScopeTag,
> {
    TextInput {
        target: TextInputTarget,
        state: TextInputState,
    },
    FileBrowser {
        target: FileBrowserTarget,
        state: FileBrowserState,
    },
    MountDstChoice {
        target: FileBrowserTarget,
        state: MountDstChoiceState,
    },
    WorkdirPick {
        state: WorkdirPickState,
    },
    Confirm {
        target: ConfirmTarget,
        state: ConfirmState,
    },
    SaveDiscardCancel {
        state: SaveDiscardState,
    },
    GithubPicker {
        state: GithubPickerState,
    },
    ConfirmSave {
        state: ConfirmSaveState,
    },
    ErrorPopup {
        state: ErrorPopupState,
    },
    ContainerInfo {
        state: ContainerInfoState,
    },
    StatusPopup {
        state: StatusPopupState,
    },
    OpPicker {
        state: Box<OpPickerState>,
    },
    RolePicker {
        state: RolePickerState,
    },
    RoleOverridePicker {
        state: RolePickerState,
    },
    AuthRolePicker {
        state: RolePickerState,
    },
    SourcePicker {
        state: SourcePickerState,
        env_key: Option<(SecretsScopeTag, String)>,
    },
    AuthSourcePicker {
        state: SourcePickerState,
    },
    ScopePicker {
        state: ScopePickerState,
    },
    AuthForm {
        target: AuthFormTarget,
        state: Box<AuthForm>,
        focus: AuthFormFocus,
        literal_buffer: String,
    },
}

impl<
    TextInputTarget,
    TextInputState,
    FileBrowserTarget,
    FileBrowserState,
    MountDstChoiceState,
    WorkdirPickState,
    ConfirmTarget,
    ConfirmState,
    SaveDiscardState,
    GithubPickerState,
    ConfirmSaveState,
    ErrorPopupState,
    ContainerInfoState,
    StatusPopupState,
    OpPickerState,
    RolePickerState,
    SourcePickerState,
    ScopePickerState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
    SecretsScopeTag,
>
    ConsoleModal<
        TextInputTarget,
        TextInputState,
        FileBrowserTarget,
        FileBrowserState,
        MountDstChoiceState,
        WorkdirPickState,
        ConfirmTarget,
        ConfirmState,
        SaveDiscardState,
        GithubPickerState,
        ConfirmSaveState,
        ErrorPopupState,
        ContainerInfoState,
        StatusPopupState,
        OpPickerState,
        RolePickerState,
        SourcePickerState,
        ScopePickerState,
        AuthFormTarget,
        AuthForm,
        AuthFormFocus,
        SecretsScopeTag,
    >
{
    #[must_use]
    pub const fn debug_kind(&self) -> crate::tui::debug::ModalDebugKind {
        use crate::tui::debug::ModalDebugKind;
        match self {
            Self::TextInput { .. } => ModalDebugKind::TextInput,
            Self::FileBrowser { .. } => ModalDebugKind::FileBrowser,
            Self::MountDstChoice { .. } => ModalDebugKind::MountDstChoice,
            Self::WorkdirPick { .. } => ModalDebugKind::WorkdirPick,
            Self::Confirm { .. } => ModalDebugKind::Confirm,
            Self::SaveDiscardCancel { .. } => ModalDebugKind::SaveDiscardCancel,
            Self::GithubPicker { .. } => ModalDebugKind::GithubPicker,
            Self::ConfirmSave { .. } => ModalDebugKind::ConfirmSave,
            Self::ErrorPopup { .. } => ModalDebugKind::ErrorPopup,
            Self::StatusPopup { .. } => ModalDebugKind::StatusPopup,
            Self::ContainerInfo { .. } => ModalDebugKind::ContainerInfo,
            Self::OpPicker { .. } => ModalDebugKind::OpPicker,
            Self::RolePicker { .. } => ModalDebugKind::RolePicker,
            Self::RoleOverridePicker { .. } => ModalDebugKind::RoleOverridePicker,
            Self::SourcePicker { .. } => ModalDebugKind::SourcePicker,
            Self::AuthSourcePicker { .. } => ModalDebugKind::AuthSourcePicker,
            Self::ScopePicker { .. } => ModalDebugKind::ScopePicker,
            Self::AuthForm { .. } => ModalDebugKind::AuthForm,
            Self::AuthRolePicker { .. } => ModalDebugKind::AuthRolePicker,
        }
    }

    #[must_use]
    pub fn rect_mode(&self, outer: Rect) -> ModalRectMode
    where
        ConfirmState: ModalConfirmState,
        GithubPickerState: ModalGithubPickerState,
        ConfirmSaveState: ModalConfirmSaveState,
        ErrorPopupState: ModalErrorPopupState,
        ContainerInfoState: ModalContainerInfoState,
        OpPickerState: ModalOpPickerState,
        RolePickerState: ModalRolePickerState,
        AuthForm: ModalAuthFormState,
    {
        match self {
            Self::TextInput { .. } => ModalRectMode::TextInput,
            Self::Confirm { state, .. } => ModalRectMode::Confirm {
                width_pct: state.width_pct(),
                height: state.required_height(),
            },
            Self::SaveDiscardCancel { .. } => ModalRectMode::SaveDiscardCancel,
            Self::FileBrowser { .. } => ModalRectMode::FileBrowser,
            Self::WorkdirPick { .. } => ModalRectMode::WorkdirPick,
            Self::MountDstChoice { .. } => ModalRectMode::MountChoice,
            Self::GithubPicker { state } => ModalRectMode::GithubPicker {
                choice_len: state.choice_len(),
            },
            Self::ConfirmSave { state } => ModalRectMode::ConfirmSave {
                required_height: state.required_height(),
            },
            Self::ErrorPopup { state } => {
                let inner_width = (outer.width * 60 / 100).saturating_sub(4);
                let max_rows = outer.height.saturating_sub(2);
                ModalRectMode::ErrorPopup {
                    required_height: state.required_height(inner_width, max_rows),
                }
            }
            Self::ContainerInfo { state } => ModalRectMode::ContainerInfo {
                required_height: state.required_height(),
            },
            Self::StatusPopup { .. } => ModalRectMode::StatusPopup,
            Self::OpPicker { state } if state.has_naming_stage_input() => ModalRectMode::TextInput,
            Self::OpPicker { .. } => ModalRectMode::OpPicker,
            Self::RolePicker { state }
            | Self::RoleOverridePicker { state }
            | Self::AuthRolePicker { state } => ModalRectMode::RolePicker {
                filtered_len: state.filtered_len(),
            },
            Self::SourcePicker { .. } | Self::AuthSourcePicker { .. } => {
                ModalRectMode::SourcePicker
            }
            Self::ScopePicker { .. } => ModalRectMode::ScopePicker,
            Self::AuthForm { state, .. } => ModalRectMode::AuthForm {
                required_height: state.required_height(),
            },
        }
    }

    #[must_use]
    pub fn rect(&self, outer: Rect) -> Rect
    where
        ConfirmState: ModalConfirmState,
        GithubPickerState: ModalGithubPickerState,
        ConfirmSaveState: ModalConfirmSaveState,
        ErrorPopupState: ModalErrorPopupState,
        ContainerInfoState: ModalContainerInfoState,
        OpPickerState: ModalOpPickerState,
        RolePickerState: ModalRolePickerState,
        AuthForm: ModalAuthFormState,
    {
        crate::tui::components::modal_rects::modal_rect_for_mode(outer, self.rect_mode(outer))
    }

    pub fn prepare_for_render(&mut self, outer: Rect)
    where
        ConfirmState: ModalConfirmState,
        GithubPickerState: ModalGithubPickerState,
        ConfirmSaveState: ModalConfirmSaveState + ModalConfirmSavePrepareState,
        ErrorPopupState: ModalErrorPopupState,
        ContainerInfoState: ModalContainerInfoState,
        OpPickerState: ModalOpPickerState,
        RolePickerState: ModalRolePickerState,
        AuthForm: ModalAuthFormState,
    {
        let modal_area = self.rect(outer);
        if let Self::ConfirmSave { state } = self {
            state.prepare_for_render(modal_area);
        }
    }

    #[must_use]
    pub fn footer_items(&self, can_generate_token: bool) -> Vec<jackin_tui::HintSpan<'static>>
    where
        FileBrowserState: ModalFileBrowserFooterState,
        ConfirmSaveState: ModalConfirmSaveFooterState,
        OpPickerState: ModalOpPickerFooterState,
        AuthForm: ModalAuthFormFooterState<AuthFormFocus>,
        AuthFormFocus: Copy,
    {
        match self {
            Self::AuthForm { state, focus, .. } => {
                crate::tui::components::footer_hints::modal_footer_items(
                    state.footer_mode(*focus, can_generate_token),
                )
            }
            Self::FileBrowser { state, .. } => state.footer_items(),
            Self::TextInput { .. } => footer_items_for_mode(ModalFooterMode::ConfirmDismiss),
            Self::MountDstChoice { .. } => footer_items_for_mode(ModalFooterMode::MountDestination),
            Self::SourcePicker { .. }
            | Self::AuthSourcePicker { .. }
            | Self::ScopePicker { .. } => footer_items_for_mode(ModalFooterMode::SegmentedChoice),
            Self::WorkdirPick { .. } => footer_items_for_mode(ModalFooterMode::PickList {
                commit_label: crate::tui::components::footer_hints::pick_list_select_footer_label(),
            }),
            Self::GithubPicker { .. } => footer_items_for_mode(ModalFooterMode::PickList {
                commit_label: crate::tui::components::footer_hints::pick_list_confirm_footer_label(
                ),
            }),
            Self::ConfirmSave { state } => footer_items_for_mode(state.footer_mode()),
            Self::SaveDiscardCancel { .. } => {
                footer_items_for_mode(ModalFooterMode::SaveDiscardCancel)
            }
            Self::ErrorPopup { .. } => footer_items_for_mode(ModalFooterMode::ErrorPopup),
            Self::ContainerInfo { .. } => footer_items_for_mode(ModalFooterMode::ContainerInfo),
            Self::StatusPopup { .. } => footer_items_for_mode(ModalFooterMode::StatusPopup),
            Self::OpPicker { state } => footer_items_for_mode(state.footer_mode(true)),
            Self::RolePicker { .. }
            | Self::RoleOverridePicker { .. }
            | Self::AuthRolePicker { .. } => {
                footer_items_for_mode(ModalFooterMode::FilteredPicker {
                    include_refresh: false,
                })
            }
            Self::Confirm { .. } => footer_items_for_mode(ModalFooterMode::YesNo),
        }
    }

    #[must_use]
    pub fn footer_items_for_area(
        &self,
        can_generate_token: bool,
        outer: Rect,
    ) -> Vec<jackin_tui::HintSpan<'static>>
    where
        FileBrowserState: ModalFileBrowserFooterState,
        ConfirmSaveState: ModalConfirmSaveFooterState,
        OpPickerState: ModalOpPickerFooterState,
        AuthForm: ModalAuthFormFooterState<AuthFormFocus>,
        AuthFormFocus: Copy,
        ConfirmState: ModalConfirmState,
        GithubPickerState: ModalGithubPickerState,
        ConfirmSaveState: ModalConfirmSaveState,
        ErrorPopupState: ModalErrorPopupState,
        ContainerInfoState: ModalContainerInfoState + ModalContainerInfoFooterState,
        RolePickerState: ModalRolePickerState,
        AuthForm: ModalAuthFormState,
        OpPickerState: ModalOpPickerState,
    {
        if let Self::ContainerInfo { state } = self {
            return crate::tui::components::footer_hints::container_info_footer_items_for_dialog(
                state.content_width(),
                state.content_height(),
                self.rect(outer),
            );
        }
        self.footer_items(can_generate_token)
    }
}

fn footer_items_for_mode(mode: ModalFooterMode) -> Vec<jackin_tui::HintSpan<'static>> {
    crate::tui::components::footer_hints::modal_footer_items(mode)
}

#[derive(Debug)]
pub struct ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache> {
    pub stage: ConsoleAppStage<Manager>,
    /// Launch input is stored as a value, not as a selected row index, so each
    /// dispatch can rebuild its current workspace choice from current config.
    pub pending_launch: Option<LaunchInput>,
    pub pending_launch_role: Option<RoleSelector>,
    /// Process-lifetime op metadata cache shared by picker instances.
    pub op_cache: OpCache,
    /// Probed once at startup; mid-session installs require restart.
    pub op_available: bool,
    /// Overlay above any sub-stage.
    pub quit_confirm: Option<jackin_tui::components::ConfirmState>,
}

#[derive(Debug)]
pub struct ConsoleCreatePreludeState<Modal> {
    pub step: CreateStep,
    pub pending_mount_src: Option<PathBuf>,
    pub pending_mount_dst: Option<String>,
    pub pending_readonly: bool,
    pub pending_workdir: Option<String>,
    pub pending_name: Option<String>,
    pub modal: Option<Modal>,
    /// Captured so Esc on `MountDstChoice` re-opens `FileBrowser` at the same
    /// directory instead of `$HOME`.
    pub last_browser_cwd: Option<PathBuf>,
    /// Picks Esc-on-`WorkdirPick` rewind target: `TextInputDst` when the
    /// Edit-destination branch was used, else `MountDstChoice`.
    pub used_edit_dst: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatePreludeCompletionStatus {
    InProgress,
    Complete,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatePreludeKeyPlan {
    Continue,
    ReturnToList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatePreludeWorkdirCancelPlan {
    ReopenTextInputDst,
    ReopenMountDstChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatePreludeMountDstChoicePlan {
    CommitSamePath,
    OpenEditInput,
    ReopenFileBrowserAtLastCwd,
    Continue,
}

#[must_use]
pub const fn create_prelude_mount_dst_choice_plan(
    outcome: jackin_tui::ModalOutcome<crate::tui::components::mount_dst_choice::MountDstChoice>,
) -> CreatePreludeMountDstChoicePlan {
    match outcome {
        jackin_tui::ModalOutcome::Commit(
            crate::tui::components::mount_dst_choice::MountDstChoice::SamePath,
        ) => CreatePreludeMountDstChoicePlan::CommitSamePath,
        jackin_tui::ModalOutcome::Commit(
            crate::tui::components::mount_dst_choice::MountDstChoice::Edit,
        ) => CreatePreludeMountDstChoicePlan::OpenEditInput,
        jackin_tui::ModalOutcome::Cancel => {
            CreatePreludeMountDstChoicePlan::ReopenFileBrowserAtLastCwd
        }
        jackin_tui::ModalOutcome::Continue => CreatePreludeMountDstChoicePlan::Continue,
    }
}

#[must_use]
pub const fn create_prelude_workdir_cancel_plan(
    used_edit_dst: bool,
) -> CreatePreludeWorkdirCancelPlan {
    if used_edit_dst {
        CreatePreludeWorkdirCancelPlan::ReopenTextInputDst
    } else {
        CreatePreludeWorkdirCancelPlan::ReopenMountDstChoice
    }
}

#[must_use]
pub const fn create_prelude_key_plan(key: crossterm::event::KeyCode) -> CreatePreludeKeyPlan {
    match key {
        crossterm::event::KeyCode::Esc => CreatePreludeKeyPlan::ReturnToList,
        _ => CreatePreludeKeyPlan::Continue,
    }
}

#[must_use]
pub const fn create_prelude_completion_status(
    modal_open: bool,
    completed: bool,
) -> CreatePreludeCompletionStatus {
    if modal_open {
        CreatePreludeCompletionStatus::InProgress
    } else if completed {
        CreatePreludeCompletionStatus::Complete
    } else {
        CreatePreludeCompletionStatus::Cancelled
    }
}

impl<Modal> Default for ConsoleCreatePreludeState<Modal> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Modal> ConsoleCreatePreludeState<Modal> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            step: CreateStep::PickFirstMountSrc,
            pending_mount_src: None,
            pending_mount_dst: None,
            pending_readonly: false,
            pending_workdir: None,
            pending_name: None,
            modal: None,
            last_browser_cwd: None,
            used_edit_dst: false,
        }
    }

    pub fn accept_mount_src(&mut self, src: PathBuf) {
        self.pending_mount_src = Some(src);
        self.step = CreateStep::PickFirstMountDst;
    }

    /// Default mount dst = same absolute path as host src. Operator can
    /// overwrite in the dst modal.
    #[must_use]
    pub fn default_mount_dst(&self) -> String {
        let src_display = self
            .pending_mount_src
            .as_ref()
            .map(|path| path.display().to_string());
        crate::tui::screens::workspaces::view::create_prelude_mount_destination_default(
            src_display.as_deref(),
        )
    }

    pub fn accept_mount_dst(&mut self, dst: String, readonly: bool) {
        self.pending_mount_dst = Some(dst);
        self.pending_readonly = readonly;
        self.step = CreateStep::PickWorkdir;
    }

    pub fn accept_workdir(&mut self, workdir: String) {
        self.pending_workdir = Some(workdir);
        self.step = CreateStep::NameWorkspace;
    }

    /// Default name = mount dst basename.
    #[must_use]
    pub fn default_name(&self) -> String {
        crate::tui::screens::workspaces::view::create_prelude_workspace_name_default(
            self.pending_mount_dst.as_deref(),
        )
    }

    pub fn accept_name(&mut self, name: String) {
        self.pending_name = Some(name);
    }

    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.pending_name.as_deref()
    }

    #[must_use]
    pub fn build_workspace(&self) -> Option<jackin_config::WorkspaceConfig> {
        let src = self.pending_mount_src.as_ref()?;
        let dst = self.pending_mount_dst.as_ref()?;
        let workdir = self.pending_workdir.as_ref()?;

        Some(jackin_config::WorkspaceConfig {
            workdir: workdir.clone(),
            mounts: vec![jackin_config::MountConfig {
                src: src.display().to_string(),
                dst: dst.clone(),
                readonly: self.pending_readonly,
                isolation: jackin_config::MountIsolation::Shared,
            }],
            ..jackin_config::WorkspaceConfig::default()
        })
    }

    #[must_use]
    pub fn completed(&self) -> Option<(String, jackin_config::WorkspaceConfig)> {
        let name = self.pending_name.clone()?;
        let workspace = self.build_workspace()?;
        Some((name, workspace))
    }
}

impl<Manager, LaunchInput, RoleSelector, OpCache>
    ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>
{
    pub fn new(stage: ConsoleAppStage<Manager>, op_cache: OpCache, op_available: bool) -> Self {
        Self {
            stage,
            pending_launch: None,
            pending_launch_role: None,
            op_cache,
            op_available,
            quit_confirm: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use jackin_config::MountIsolation;
    use ratatui::layout::Rect;

    use crate::tui::components::footer_hints::{
        ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalContainerInfoFooterState,
        ModalFileBrowserFooterState, ModalFooterMode, ModalOpPickerFooterState,
    };
    use crate::tui::components::modal_rects::{
        ModalAuthFormState, ModalConfirmSaveState, ModalConfirmState, ModalContainerInfoState,
        ModalErrorPopupState, ModalGithubPickerState, ModalOpPickerState, ModalRectMode,
        ModalRolePickerState,
    };

    use super::{
        ConsoleCreatePreludeState, ConsoleManagerStage, ConsoleManagerStageRoute, ConsoleModal,
        CreatePreludeCompletionStatus, CreatePreludeKeyPlan, CreatePreludeMountDstChoicePlan,
        CreatePreludeWorkdirCancelPlan, create_prelude_completion_status, create_prelude_key_plan,
        create_prelude_mount_dst_choice_plan, create_prelude_workdir_cancel_plan,
    };

    struct TestConfirm;

    impl ModalConfirmState for TestConfirm {
        fn width_pct(&self) -> u16 {
            42
        }

        fn required_height(&self) -> u16 {
            9
        }
    }

    #[test]
    fn console_manager_stage_routes_by_variant() {
        assert_eq!(
            ConsoleManagerStage::<(), (), ()>::List.route(),
            ConsoleManagerStageRoute::List
        );
        assert_eq!(
            ConsoleManagerStage::<(), (), ()>::Editor(()).route(),
            ConsoleManagerStageRoute::Editor
        );
        assert_eq!(
            ConsoleManagerStage::<(), (), ()>::Settings(()).route(),
            ConsoleManagerStageRoute::Settings
        );
        assert_eq!(
            ConsoleManagerStage::<(), (), ()>::CreatePrelude(()).route(),
            ConsoleManagerStageRoute::CreatePrelude
        );
        assert_eq!(
            ConsoleManagerStage::<(), (), ()>::ConfirmDelete {
                name: "workspace".to_owned(),
                state: jackin_tui::components::ConfirmState::new("Delete?"),
            }
            .route(),
            ConsoleManagerStageRoute::ConfirmDelete
        );
        assert_eq!(
            ConsoleManagerStage::<(), (), ()>::ConfirmInstancePurge {
                container: "container".to_owned(),
                label: "label".to_owned(),
                state: jackin_tui::components::ConfirmState::new("Purge?"),
            }
            .route(),
            ConsoleManagerStageRoute::ConfirmInstancePurge
        );
    }

    #[test]
    fn create_prelude_completion_status_routes_modal_complete_and_cancel() {
        assert_eq!(
            create_prelude_completion_status(true, true),
            CreatePreludeCompletionStatus::InProgress
        );
        assert_eq!(
            create_prelude_completion_status(false, true),
            CreatePreludeCompletionStatus::Complete
        );
        assert_eq!(
            create_prelude_completion_status(false, false),
            CreatePreludeCompletionStatus::Cancelled
        );
    }

    #[test]
    fn create_prelude_key_plan_routes_escape_to_list() {
        assert_eq!(
            create_prelude_key_plan(crossterm::event::KeyCode::Esc),
            CreatePreludeKeyPlan::ReturnToList
        );
        assert_eq!(
            create_prelude_key_plan(crossterm::event::KeyCode::Enter),
            CreatePreludeKeyPlan::Continue
        );
    }

    #[test]
    fn create_prelude_workdir_cancel_plan_reopens_prior_dst_step() {
        assert_eq!(
            create_prelude_workdir_cancel_plan(true),
            CreatePreludeWorkdirCancelPlan::ReopenTextInputDst
        );
        assert_eq!(
            create_prelude_workdir_cancel_plan(false),
            CreatePreludeWorkdirCancelPlan::ReopenMountDstChoice
        );
    }

    #[test]
    fn create_prelude_mount_dst_choice_plan_routes_choice_outcomes() {
        use crate::tui::components::mount_dst_choice::MountDstChoice;

        assert_eq!(
            create_prelude_mount_dst_choice_plan(jackin_tui::ModalOutcome::Commit(
                MountDstChoice::SamePath
            )),
            CreatePreludeMountDstChoicePlan::CommitSamePath
        );
        assert_eq!(
            create_prelude_mount_dst_choice_plan(jackin_tui::ModalOutcome::Commit(
                MountDstChoice::Edit
            )),
            CreatePreludeMountDstChoicePlan::OpenEditInput
        );
        assert_eq!(
            create_prelude_mount_dst_choice_plan(jackin_tui::ModalOutcome::Cancel),
            CreatePreludeMountDstChoicePlan::ReopenFileBrowserAtLastCwd
        );
        assert_eq!(
            create_prelude_mount_dst_choice_plan(jackin_tui::ModalOutcome::Continue),
            CreatePreludeMountDstChoicePlan::Continue
        );
    }

    struct TestGithubPicker(usize);

    impl ModalGithubPickerState for TestGithubPicker {
        fn choice_len(&self) -> usize {
            self.0
        }
    }

    struct TestConfirmSave;

    impl ModalConfirmSaveState for TestConfirmSave {
        fn required_height(&self) -> u16 {
            12
        }
    }

    impl ModalConfirmSaveFooterState for TestConfirmSave {
        fn footer_mode(&self) -> ModalFooterMode {
            ModalFooterMode::ConfirmSave {
                scroll_axes: jackin_tui::components::ScrollAxes::none(),
            }
        }
    }

    struct TestError;

    impl ModalErrorPopupState for TestError {
        fn required_height(&self, _inner_width: u16, _max_rows: u16) -> u16 {
            14
        }
    }

    struct TestContainerInfo;

    impl ModalContainerInfoState for TestContainerInfo {
        fn required_height(&self) -> u16 {
            15
        }
    }

    impl ModalContainerInfoFooterState for TestContainerInfo {
        fn content_width(&self) -> usize {
            80
        }

        fn content_height(&self) -> usize {
            40
        }
    }

    struct TestOpPicker(bool);

    impl ModalOpPickerState for TestOpPicker {
        fn has_naming_stage_input(&self) -> bool {
            self.0
        }
    }

    impl ModalOpPickerFooterState for TestOpPicker {
        fn footer_mode(&self, include_refresh: bool) -> ModalFooterMode {
            ModalFooterMode::FilteredPicker { include_refresh }
        }
    }

    struct TestRolePicker(usize);

    impl ModalRolePickerState for TestRolePicker {
        fn filtered_len(&self) -> usize {
            self.0
        }
    }

    struct TestAuthForm;

    impl ModalAuthFormState for TestAuthForm {
        fn required_height(&self) -> u16 {
            13
        }
    }

    impl ModalAuthFormFooterState<()> for TestAuthForm {
        fn footer_mode(&self, _focus: (), can_generate_token: bool) -> ModalFooterMode {
            ModalFooterMode::AuthForm {
                focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
                shows_source_folder: false,
                shows_credential_block: false,
                can_generate_token,
            }
        }
    }

    struct TestFileBrowser;

    impl ModalFileBrowserFooterState for TestFileBrowser {
        fn footer_items(&self) -> Vec<jackin_tui::HintSpan<'static>> {
            vec![jackin_tui::HintSpan::Text("file")]
        }
    }

    type RectTestModal = ConsoleModal<
        (),
        (),
        (),
        TestFileBrowser,
        (),
        (),
        (),
        TestConfirm,
        (),
        TestGithubPicker,
        TestConfirmSave,
        TestError,
        TestContainerInfo,
        (),
        TestOpPicker,
        TestRolePicker,
        (),
        (),
        (),
        TestAuthForm,
        (),
        (),
    >;

    #[test]
    fn create_prelude_completed_requires_name_and_mount_fields() {
        let mut prelude = ConsoleCreatePreludeState::<()>::new();
        prelude.accept_mount_src(PathBuf::from("/host/proj"));
        prelude.accept_mount_dst("/work/proj".into(), true);
        prelude.accept_workdir("/work/proj".into());

        assert!(prelude.completed().is_none());

        prelude.accept_name("proj".into());
        let (name, workspace) = prelude.completed().expect("complete prelude");

        assert_eq!(name, "proj");
        assert_eq!(workspace.workdir, "/work/proj");
        assert_eq!(workspace.mounts.len(), 1);
        assert_eq!(workspace.mounts[0].src, "/host/proj");
        assert_eq!(workspace.mounts[0].dst, "/work/proj");
        assert!(workspace.mounts[0].readonly);
        assert_eq!(workspace.mounts[0].isolation, MountIsolation::Shared);
    }

    #[test]
    fn console_modal_reports_debug_kind() {
        type TestModal = ConsoleModal<
            &'static str,
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
        >;

        let modal = TestModal::TextInput {
            target: "name",
            state: (),
        };

        assert_eq!(
            modal.debug_kind(),
            crate::tui::debug::ModalDebugKind::TextInput
        );
    }

    #[test]
    fn console_modal_reports_rect_mode() {
        let modal = RectTestModal::RolePicker {
            state: TestRolePicker(5),
        };

        assert_eq!(
            modal.rect_mode(Rect::new(0, 0, 100, 40)),
            ModalRectMode::RolePicker { filtered_len: 5 }
        );
    }

    #[test]
    fn console_modal_error_rect_mode_uses_required_height() {
        let modal = RectTestModal::ErrorPopup { state: TestError };

        assert_eq!(
            modal.rect_mode(Rect::new(0, 0, 100, 40)),
            ModalRectMode::ErrorPopup {
                required_height: 14
            }
        );
    }

    #[test]
    fn console_modal_reports_footer_items() {
        let modal = RectTestModal::RolePicker {
            state: TestRolePicker(5),
        };

        assert!(
            modal
                .footer_items(false)
                .contains(&jackin_tui::HintSpan::Text("filter"))
        );
    }

    #[test]
    fn console_modal_footer_items_for_area_reflects_container_info_overflow() {
        let modal = RectTestModal::ContainerInfo {
            state: TestContainerInfo,
        };

        assert!(
            modal
                .footer_items_for_area(false, Rect::new(0, 0, 100, 20))
                .contains(&jackin_tui::HintSpan::Text("scroll"))
        );
    }
}
