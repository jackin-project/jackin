//! Top-level console TUI app model.

use std::path::PathBuf;

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
