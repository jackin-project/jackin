// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ConsoleManagerMessage` vocabulary.

use ratatui::layout::Rect;
use std::path::PathBuf;

#[derive(Debug)]
pub enum ConsoleManagerMessage<
    CreatePrelude,
    Editor,
    Settings,
    FileBrowserCommit,
    FileBrowserListing,
    InstanceRefreshSnapshot,
    MountInfoRefresh,
    OpRef,
    AppConfig,
    WorkspaceConfig,
    EditorTab,
    SettingsTab,
    SecretsScopeTag,
    MountScrollFocus,
    DragState,
    ContainerInfoState,
    GithubPickerState,
> {
    CollapseSelectedTree,
    EnterPreview,
    EnterConfirmDelete {
        name: String,
    },
    EnterConfirmInstancePurge {
        container: String,
        label: String,
    },
    EnterCreateEditor {
        name: String,
        workspace: WorkspaceConfig,
    },
    EnterCreatePrelude(CreatePrelude),
    EnterEditor(Editor),
    EnterSettings(Settings),
    FileBrowserCommitValidated(FileBrowserCommit),
    FileBrowserListingLoaded(FileBrowserListing),
    InstancesRefreshed(Result<InstanceRefreshSnapshot, String>),
    MountInfoRefreshed(MountInfoRefresh),
    OpCommitResolved {
        op_ref: OpRef,
        result: anyhow::Result<()>,
        is_settings: bool,
    },
    PollFileBrowserGitUrls,
    PollPickerLoads,
    FocusEditorContent,
    FocusEditorTabBar,
    FocusSettingsContent,
    FocusSettingsTabBar,
    ExitPreview,
    ExpandSelectedTree,
    ClearSettingsAuthKind,
    DismissSettingsErrorPopup,
    OpenSettingsErrorPopup {
        title: String,
        message: String,
    },
    EnterSettingsAuthKind,
    ScrollEditorTabHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    SelectEditorMountRow(usize),
    SelectEditorTab(EditorTab),
    SelectListRow(usize),
    SelectSettingsTab(SettingsTab),
    SelectSettingsTrustRow(usize),
    ScrollEditorWorkspaceMountsHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    ScrollSettingsGlobalMountsHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    ScrollSettingsTrustHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    MoveSettingsGlobalMountsSelection {
        delta: isize,
        term: Rect,
        footer_h: u16,
    },
    MoveSettingsEnvSelection {
        delta: isize,
        term: Rect,
        footer_h: u16,
    },
    MoveSettingsTrustSelection {
        delta: isize,
        term: Rect,
        footer_h: u16,
    },
    MoveEditorTab {
        delta: isize,
        focus_tab_bar: bool,
    },
    MoveEditorFieldSelection {
        delta: isize,
        max_row: usize,
        skipped_rows: Vec<usize>,
        term: Rect,
        footer_h: u16,
    },
    MoveSettingsTab {
        delta: isize,
        focus_tab_bar: bool,
    },
    MoveSettingsGeneralSelection {
        delta: isize,
    },
    MoveSettingsAuthSelection {
        delta: isize,
    },
    SetSettingsEnvRoleExpanded {
        role: String,
        expanded: bool,
    },
    SetEditorSecretsRoleExpanded {
        role: String,
        expanded: bool,
    },
    ToggleSettingsGlobalMountReadonly,
    ToggleEditorGeneralSelected,
    ToggleEditorMountReadonlySelected,
    ToggleEditorSecretMask {
        scope: SecretsScopeTag,
        key: String,
    },
    ToggleSettingsGeneralSelected,
    ToggleSettingsTrustSelected,
    MoveListSelection(isize),
    MovePreviewPane {
        container: String,
        delta: isize,
    },
    ReloadFromConfig {
        config: Box<AppConfig>,
        cwd: PathBuf,
    },
    ReturnToList,
    ScrollListHorizontal(i16),
    ScrollFocusedListBlockVertical(i16),
    SetListScrollFocus(Option<MountScrollFocus>),
    SetListNamesFocused(bool),
    SetDragState(Option<DragState>),
    SetListSplitPct(u16),
    OpenListErrorPopup {
        title: String,
        message: String,
    },
    OpenStatusPopup {
        title: String,
        message: String,
    },
    DismissStatusPopup,
    OpenListContainerInfo {
        state: ContainerInfoState,
    },
    OpenListGithubPicker {
        state: GithubPickerState,
    },
    DismissListModal,
    DismissInlineSessionPicker,
    DismissInlineRolePicker,
    DismissInlineAgentPicker,
    DismissInlineAccountPicker,
    DismissLaunchAccountPicker,
    Settings(crate::tui::screens::settings::message::SettingsMessage),
}
