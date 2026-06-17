//! Top-level console TUI app model.

use std::path::PathBuf;

use ratatui::layout::Rect;

use crate::tui::components::modal_rects::{
    ModalAuthFormState, ModalConfirmSaveState, ModalConfirmState, ModalContainerInfoState,
    ModalErrorPopupState, ModalGithubPickerState, ModalOpPickerState, ModalRectMode,
    ModalRolePickerState,
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

    use crate::tui::components::modal_rects::{
        ModalAuthFormState, ModalConfirmSaveState, ModalConfirmState, ModalContainerInfoState,
        ModalErrorPopupState, ModalGithubPickerState, ModalOpPickerState, ModalRectMode,
        ModalRolePickerState,
    };

    use super::{ConsoleCreatePreludeState, ConsoleModal};

    struct TestConfirm;

    impl ModalConfirmState for TestConfirm {
        fn width_pct(&self) -> u16 {
            42
        }

        fn required_height(&self) -> u16 {
            9
        }
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

    struct TestOpPicker(bool);

    impl ModalOpPickerState for TestOpPicker {
        fn has_naming_stage_input(&self) -> bool {
            self.0
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

    type RectTestModal = ConsoleModal<
        (),
        (),
        (),
        (),
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
}
