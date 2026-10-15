// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleEffect {
    RequestActiveMountInfoRefresh,
    RequestInstanceRefresh,
    RequestUsageRefresh,
    RequestWorkflowsRefresh,
    SaveSettings,
}

#[derive(Debug)]
pub enum ConsoleManagerEffect<RoleSelector, RoleSource, OpRef> {
    Console(ConsoleEffect),
    StartRoleRegistration {
        raw: String,
        key: String,
        selector: RoleSelector,
        source: RoleSource,
    },
    PersistTrustedRoleSource {
        key: String,
        source: RoleSource,
    },
    OpenCreatePreludeFileBrowser,
    OpenCreatePreludeFileBrowserAtLastCwd,
    OpenEditorAuthSourceFolderBrowser,
    OpenEditorAddMountFileBrowser,
    OpenGlobalMountFileBrowser,
    OpenSettingsAuthSourceFolderBrowser,
    ApplyFileBrowserOutcome {
        context: FileBrowserEffectContext,
        outcome: crate::tui::components::file_browser::FileBrowserOutcome<std::path::PathBuf>,
    },
    ResolveFileBrowserGitUrl(std::path::PathBuf),
    PollFileBrowserGitUrls,
    PollPickerLoads,
    CopyContainerInfoValue {
        row: usize,
        payload: String,
    },
    OpenUrl(String),
    RemoveWorkspace {
        name: String,
        cwd: std::path::PathBuf,
    },
    ValidateOpCommit {
        op_ref: OpRef,
        is_settings: bool,
    },
    StartAccountScan {
        generation: u64,
    },
    /// Host-executed workflow intent from the Workflows screen
    /// (resume/cancel/takeover/release/observe/export). The host
    /// performs the daemon and Capsule calls, then refreshes the
    /// screen snapshots.
    Workflows(crate::tui::screens::workflows::effect::WorkflowsEffect),
}

#[derive(Debug, Clone)]
pub enum FileBrowserEffectContext {
    Editor,
    Prelude {
        browser_cwd: Option<std::path::PathBuf>,
    },
    SettingsMounts,
    SettingsAuth,
}

#[derive(Debug)]
pub enum WorkspaceSaveEffect<MountConfig, PendingSaveCommit, IsolationRecord, WorkspaceConfig> {
    StartDriftCheck {
        original_name: String,
        prospective_mounts: Vec<MountConfig>,
        plan: PendingSaveCommit,
        exit_on_success: bool,
    },
    StartIsolationCleanup {
        records: Vec<IsolationRecord>,
        plan: PendingSaveCommit,
        exit_on_success: bool,
    },
    WriteWorkspace {
        mode: WorkspaceSaveWriteMode,
        original: WorkspaceConfig,
        pending: WorkspaceConfig,
        exit_on_success: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceSaveWriteMode {
    Edit {
        original_name: String,
        pending_name: Option<String>,
        effective_removals: Vec<String>,
    },
    Create {
        name: String,
    },
}

#[derive(Debug)]
pub struct WorkspaceSaveWriteInput<'a, WorkspaceConfig> {
    pub mode: WorkspaceSaveWriteMode,
    pub original: &'a WorkspaceConfig,
    pub pending: &'a WorkspaceConfig,
}

impl<RoleSelector, RoleSource, OpRef> From<ConsoleEffect>
    for ConsoleManagerEffect<RoleSelector, RoleSource, OpRef>
{
    fn from(effect: ConsoleEffect) -> Self {
        Self::Console(effect)
    }
}
