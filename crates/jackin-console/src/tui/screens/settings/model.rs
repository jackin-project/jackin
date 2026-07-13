//! Settings screen state: per-tab state structs for the General, Mounts,
//! Environments, Auth, and Trust panels.

#[path = "model/auth_impls.rs"]
mod auth_impls;
#[path = "model/env_impls.rs"]
mod env_impls;
#[path = "model/general_impls.rs"]
mod general_impls;
#[path = "model/trust_impls.rs"]
mod trust_impls;

#[allow(
    unused_imports,
    unreachable_pub,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub use auth_impls::*;
#[allow(
    unused_imports,
    unreachable_pub,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub use env_impls::*;
#[allow(
    unused_imports,
    unreachable_pub,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub use general_impls::*;
#[allow(
    unused_imports,
    unreachable_pub,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub use trust_impls::*;

// Not responsible for: event handling (see `update`) or rendering (see
// `view`).

use std::collections::BTreeMap;

use crate::tui::auth::{AuthKind, AuthMode};
use crate::tui::components::footer_hints::{
    ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalFileBrowserFooterState,
    ModalFooterMode, ModalOpPickerFooterState,
};
use crate::tui::components::modal_rects::{
    ModalAuthFormState, ModalConfirmSavePrepareState, ModalConfirmSaveState, ModalConfirmState,
    ModalOpPickerState, ModalRectMode, ModalRolePickerState,
};
use jackin_tui::components::{FocusOwner, ModalStack};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Mounts,
    Environments,
    Auth,
    Trust,
}

impl SettingsTab {
    pub const ALL: [Self; 5] = [
        Self::General,
        Self::Mounts,
        Self::Environments,
        Self::Auth,
        Self::Trust,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Mounts => "Mounts",
            Self::Environments => "Environments",
            Self::Auth => "Auth",
            Self::Trust => "Trust",
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    #[must_use]
    pub fn previous(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug)]
pub struct SettingsState<Mounts, Env, Auth, Trust, ErrorPopup, PendingToken> {
    pub active_tab: SettingsTab,
    /// W3C ARIA Tabs: focus is either on the tab list or the active tab panel.
    pub focus_owner: FocusOwner<SettingsTab>,
    pub hover_target: Option<SettingsHoverTarget>,
    pub general: SettingsGeneralState,
    pub mounts: Mounts,
    pub env: Env,
    pub auth: Auth,
    pub trust: Trust,
    /// Error popup shown on top of all settings content.
    pub error_popup: Option<ErrorPopup>,
    /// Token-generate request drained by the run loop.
    pub pending_token_generate: Option<PendingToken>,
    /// Cached footer height for mouse hit-testing.
    pub cached_footer_h: u16,
}

pub trait SettingsPanelTakeError {
    fn take_panel_error(&mut self) -> Option<String>;
}

pub trait SettingsAuthRestorePendingForm {
    fn restore_pending_auth_form(&mut self);
}

pub trait SettingsMountsTakeExit {
    fn take_mounts_exit_requested(&mut self) -> bool;
}

pub trait SettingsModalSlot {
    type Modal;

    fn modal_mut(&mut self) -> Option<&mut Self::Modal>;
}

pub trait SettingsAuthSlot {
    type Modal;

    fn modal_mut(&mut self) -> Option<&mut Self::Modal>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsAfterEventOutcome {
    pub exit_requested: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHoverTarget {
    Tab(usize),
    TrustRow(usize),
}

impl<Mounts, Env, Auth, Trust, ErrorPopup, PendingToken>
    SettingsState<Mounts, Env, Auth, Trust, ErrorPopup, PendingToken>
{
    pub fn dismiss_error_popup(&mut self)
    where
        Auth: SettingsAuthRestorePendingForm,
    {
        self.error_popup = None;
        self.auth.restore_pending_auth_form();
    }

    #[must_use]
    pub const fn focus_owner(&self) -> FocusOwner<SettingsTab> {
        self.focus_owner
    }

    #[must_use]
    pub const fn content_area(&self, term_size: ratatui::layout::Rect) -> ratatui::layout::Rect {
        crate::tui::layout::tabbed_content_area(term_size, self.cached_footer_h)
    }

    pub fn set_focus_owner(&mut self, owner: FocusOwner<SettingsTab>) {
        self.focus_owner = owner;
    }

    pub fn apply_tab_move_plan(
        &mut self,
        plan: crate::tui::screens::settings::update::SettingsTabMovePlan,
    ) {
        self.active_tab = plan.active_tab;
        self.set_tab_bar_focused(plan.tab_bar_focused);
    }

    #[must_use]
    pub const fn tab_bar_focused(&self) -> bool {
        self.focus_owner.is_tab_bar()
    }

    pub fn set_tab_bar_focused(&mut self, focused: bool) {
        self.focus_owner = if focused {
            FocusOwner::TabBar
        } else {
            FocusOwner::Content(self.active_tab)
        };
    }

    pub fn apply_tab_bar_focus_plan(&mut self, focused: bool) {
        self.set_tab_bar_focused(focused);
    }

    #[must_use]
    pub fn content_focused(&self, tab: SettingsTab) -> bool {
        self.focus_owner == FocusOwner::Content(tab)
    }

    pub fn set_content_focused(&mut self, tab: SettingsTab, focused: bool) {
        if focused {
            self.focus_owner = FocusOwner::Content(tab);
        } else if self.content_focused(tab) {
            self.focus_owner = FocusOwner::TabBar;
        }
    }

    pub fn apply_scroll_focus_plan(
        &mut self,
        plan: crate::tui::screens::settings::update::SettingsScrollFocusPlan,
    ) {
        self.set_content_focused(SettingsTab::Mounts, plan.mounts);
        self.set_content_focused(SettingsTab::Environments, plan.env);
        self.set_content_focused(SettingsTab::Auth, plan.auth);
        self.set_content_focused(SettingsTab::Trust, plan.trust);
    }

    pub fn set_active_content_focused(&mut self, focused: bool) {
        self.set_content_focused(self.active_tab, focused);
    }

    #[must_use]
    pub const fn hovered_tab(&self) -> Option<usize> {
        match self.hover_target {
            Some(SettingsHoverTarget::Tab(index)) => Some(index),
            _ => None,
        }
    }

    #[must_use]
    pub const fn hovered_trust_row(&self) -> Option<usize> {
        match self.hover_target {
            Some(SettingsHoverTarget::TrustRow(index)) => Some(index),
            _ => None,
        }
    }

    pub fn set_hover_target(&mut self, target: Option<SettingsHoverTarget>) {
        self.hover_target = target;
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool
    where
        SettingsGeneralState: SettingsPanelDirty,
        Mounts: SettingsPanelDirty,
        Env: SettingsPanelDirty,
        Auth: SettingsPanelDirty,
        Trust: SettingsPanelDirty,
    {
        self.general.panel_is_dirty()
            || self.mounts.panel_is_dirty()
            || self.env.panel_is_dirty()
            || self.auth.panel_is_dirty()
            || self.trust.panel_is_dirty()
    }

    #[must_use]
    pub fn change_count(&self) -> usize
    where
        SettingsGeneralState: SettingsPanelChangeCount,
        Mounts: SettingsPanelChangeCount,
        Env: SettingsPanelChangeCount,
        Auth: SettingsPanelChangeCount,
        Trust: SettingsPanelChangeCount,
    {
        self.general.panel_change_count()
            + self.mounts.panel_change_count()
            + self.env.panel_change_count()
            + self.auth.panel_change_count()
            + self.trust.panel_change_count()
    }

    pub fn discard_all(&mut self)
    where
        SettingsGeneralState: SettingsPanelDiscard,
        Mounts: SettingsPanelDiscard,
        Env: SettingsPanelDiscard,
        Auth: SettingsPanelDiscard,
        Trust: SettingsPanelDiscard,
    {
        self.general.panel_discard();
        self.mounts.panel_discard();
        self.env.panel_discard();
        self.auth.panel_discard();
        self.trust.panel_discard();
        self.pending_token_generate = None;
    }

    pub fn mark_saved(&mut self)
    where
        SettingsGeneralState: SettingsPanelMarkSaved,
        Mounts: SettingsPanelMarkSaved,
        Env: SettingsPanelMarkSaved,
        Auth: SettingsPanelMarkSaved,
        Trust: SettingsPanelMarkSaved,
    {
        self.general.panel_mark_saved();
        self.mounts.panel_mark_saved();
        self.env.panel_mark_saved();
        self.auth.panel_mark_saved();
        self.trust.panel_mark_saved();
    }
}

impl<Mounts, Env, Auth, Trust, ErrorPopup, PendingToken>
    crate::tui::model::ConsolePendingTokenGenerate
    for SettingsState<Mounts, Env, Auth, Trust, ErrorPopup, PendingToken>
{
    type PendingTokenGenerate = PendingToken;

    fn take_pending_token_generate(&mut self) -> Option<Self::PendingTokenGenerate> {
        self.pending_token_generate.take()
    }
}

impl<Mounts, Env, Auth, Trust, PendingToken>
    SettingsState<Mounts, Env, Auth, Trust, jackin_tui::components::ErrorPopupState, PendingToken>
{
    pub fn open_error_popup(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.error_popup = Some(crate::tui::components::error_popup::error_popup_state(
            title, message,
        ));
    }
}

impl<Mounts, Env, Auth, Trust, ErrorPopup, PendingToken> crate::tui::model::ConsolePendingOpCommit
    for SettingsState<Mounts, Env, Auth, Trust, ErrorPopup, PendingToken>
where
    Auth: crate::tui::model::ConsolePendingOpCommit,
{
    type OpRef = Auth::OpRef;

    fn poll_pending_op_commit(&mut self) -> Option<(Self::OpRef, anyhow::Result<()>)> {
        self.auth.poll_pending_op_commit()
    }
}

impl<Mounts, Env, Auth, Trust, ErrorPopup, PendingToken> crate::tui::model::ConsoleAnimationTick
    for SettingsState<Mounts, Env, Auth, Trust, ErrorPopup, PendingToken>
where
    Env: SettingsModalSlot,
    Env::Modal: crate::tui::model::ConsoleAnimationTick,
    Auth: SettingsAuthSlot,
    Auth::Modal: crate::tui::model::ConsoleAnimationTick,
{
    fn tick_active_animation(&mut self) -> bool {
        let mut dirty = false;
        if let Some(modal) = self.env.modal_mut() {
            dirty |= modal.tick_active_animation();
        }
        if let Some(modal) = self.auth.modal_mut() {
            dirty |= modal.tick_active_animation();
        }
        dirty
    }
}

impl<Mounts, Env, Auth, Trust, ErrorPopup, PendingToken>
    SettingsState<Mounts, Env, Auth, Trust, ErrorPopup, PendingToken>
where
    Mounts: SettingsMountsTakeExit + SettingsPanelTakeError,
    Env: SettingsPanelTakeError,
    Auth: SettingsPanelTakeError,
    Trust: SettingsPanelTakeError,
{
    pub fn take_after_event_outcome(&mut self) -> SettingsAfterEventOutcome {
        let error = self
            .mounts
            .take_panel_error()
            .or_else(|| self.env.take_panel_error())
            .or_else(|| self.auth.take_panel_error())
            .or_else(|| self.trust.take_panel_error());
        let exit_requested = self.mounts.take_mounts_exit_requested();
        SettingsAfterEventOutcome {
            exit_requested,
            error,
        }
    }
}

impl<Mounts, EnvModal, AuthModal, PendingOpCommit, Trust, ErrorPopup, PendingToken>
    SettingsState<
        Mounts,
        SettingsEnvState<jackin_config::EnvValue, EnvModal>,
        SettingsAuthState<jackin_config::EnvValue, AuthModal, PendingOpCommit>,
        Trust,
        ErrorPopup,
        PendingToken,
    >
{
    pub fn clear_ignored_env_only_auth_keys(&mut self) {
        crate::tui::auth_config::clear_ignored_env_only_settings_auth_keys(
            &self.auth.pending,
            &mut self.env.pending.env,
        );
    }
}

impl<Mounts, EnvValue, EnvModal, Auth, Trust, ErrorPopup, PendingToken>
    SettingsState<
        Mounts,
        SettingsEnvState<EnvValue, EnvModal>,
        Auth,
        Trust,
        ErrorPopup,
        PendingToken,
    >
{
    #[must_use]
    pub fn env_flat_rows(&self) -> Vec<SettingsEnvRow> {
        crate::tui::screens::settings::update::settings_env_flat_rows(
            &self.env.pending,
            &self.env.expanded,
        )
    }
}

impl<MountModal, EnvModal, AuthModal, PendingOpCommit, ErrorPopup, PendingToken>
    SettingsState<
        GlobalMountsState<jackin_config::GlobalMountRow, MountModal>,
        SettingsEnvState<jackin_config::EnvValue, EnvModal>,
        SettingsAuthState<jackin_config::EnvValue, AuthModal, PendingOpCommit>,
        SettingsTrustState,
        ErrorPopup,
        PendingToken,
    >
{
    #[must_use]
    pub fn from_config(config: &jackin_config::AppConfig) -> Self {
        Self {
            active_tab: SettingsTab::General,
            focus_owner: FocusOwner::TabBar,
            hover_target: None,
            general: SettingsGeneralState::from_values(config.git.coauthor_trailer, config.git.dco),
            mounts: GlobalMountsState::from_rows(config.list_mount_rows()),
            env: SettingsEnvState::from_config(config),
            auth: SettingsAuthState::from_config(config),
            trust: SettingsTrustState::from_config(config),
            error_popup: None,
            pending_token_generate: None,
            cached_footer_h: 1,
        }
    }

    pub fn clamp_mounts_scroll_for_frame(&mut self, area: ratatui::layout::Rect) {
        crate::tui::screens::settings::view::clamp_mounts_scroll_x_for_frame(
            area,
            crate::tui::mount_display::settings_global_config_mounts_content_width_with_cache(
                &self.mounts.pending,
                &self.mounts.mount_info_cache,
            ),
            &mut self.mounts.scroll_x,
        );
    }

    pub fn apply_trust_row_select_plan(
        &mut self,
        plan: crate::tui::screens::settings::update::SettingsTrustRowSelectPlan,
    ) {
        let content_focused = self.trust.apply_row_select_plan(plan);
        self.set_content_focused(SettingsTab::Trust, content_focused);
    }

    #[must_use]
    pub fn mounts_content_height(&self) -> usize {
        crate::tui::screens::settings::view::mounts_content_height(
            crate::tui::mount_display::settings_global_config_mounts_content_height(
                &self.mounts.pending,
            ),
            self.mounts.error.is_some(),
        )
    }

    #[must_use]
    pub fn env_content_height(&self) -> usize {
        crate::tui::screens::settings::view::env_content_height(
            self.env_flat_rows().len(),
            self.env.error.is_some(),
        )
    }

    #[must_use]
    pub fn auth_content_height(&self) -> usize {
        crate::tui::screens::settings::view::auth_content_height(
            self.auth.selected_kind,
            &self.auth.pending,
            |kind, mode| {
                crate::tui::screens::settings::update::settings_auth_detail_row_count(kind, *mode)
            },
            self.auth.error.is_some(),
        )
    }

    #[must_use]
    pub fn trust_content_height(&self) -> usize {
        crate::tui::screens::settings::view::trust_content_height(
            self.trust.pending.len(),
            self.trust.error.is_some(),
        )
    }
}

pub trait SettingsPanelDirty {
    fn panel_is_dirty(&self) -> bool;
}

pub trait SettingsPanelChangeCount {
    fn panel_change_count(&self) -> usize;
}

pub trait SettingsPanelDiscard {
    fn panel_discard(&mut self);
}

pub trait SettingsPanelMarkSaved {
    fn panel_mark_saved(&mut self);
}

/// Cursor position inside the auth-edit form modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFormFocus {
    Mode,
    SourceFolder,
    CredentialSource,
    Save,
    Cancel,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthFormTarget<K> {
    Workspace { kind: K },
    WorkspaceRole { role: String, kind: K },
}

impl<K> AuthFormTarget<K> {
    pub const fn kind(&self) -> &K {
        match self {
            Self::Workspace { kind } | Self::WorkspaceRole { kind, .. } => kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsAuthRow<K, M> {
    pub kind: K,
    pub mode: M,
    pub sync_source_dir: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettingsEnvScope {
    Global,
    Role(String),
}

#[derive(Debug, Clone)]
pub enum SettingsEnvRow {
    Key {
        scope: SettingsEnvScope,
        key: String,
    },
    GlobalAddSentinel,
    RoleHeader {
        role: String,
        expanded: bool,
    },
    RoleAddSentinel(String),
    SectionSpacer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEnvConfirm {
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvTextTarget {
    EnvKey {
        scope: SettingsEnvScope,
    },
    EnvValue {
        scope: SettingsEnvScope,
        key: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvEnterPlan {
    EditValue {
        scope: SettingsEnvScope,
        key: String,
    },
    OpenScopePicker,
    ExpandRole(String),
    AddRoleKey {
        scope: SettingsEnvScope,
    },
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvOpPickerTarget {
    Existing {
        scope: SettingsEnvScope,
        key: String,
    },
    NewKey {
        scope: SettingsEnvScope,
    },
}

#[derive(Debug)]
pub enum SettingsModal<
    EnvValue,
    TextInputState,
    SourcePickerState,
    OpPickerState,
    FileBrowserState,
    MountDstChoiceState,
    RolePickerState,
    ScopePickerState,
    ConfirmState,
    ConfirmSaveState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
> {
    MountText {
        target: GlobalMountTextTarget,
        state: Box<TextInputState>,
    },
    MountFileBrowser {
        state: Box<FileBrowserState>,
    },
    MountDstChoice {
        state: MountDstChoiceState,
    },
    MountScopePicker {
        state: ScopePickerState,
    },
    MountRolePicker {
        state: RolePickerState,
    },
    MountConfirm {
        action: GlobalMountConfirm,
        state: ConfirmState,
    },
    MountPreviewSave {
        state: ConfirmSaveState,
    },
    EnvText {
        target: SettingsEnvTextTarget,
        pending_value: Option<EnvValue>,
        state: Box<TextInputState>,
    },
    EnvSourcePicker {
        key: (SettingsEnvScope, String),
        state: SourcePickerState,
    },
    EnvOpPicker {
        target: SettingsEnvOpPickerTarget,
        state: Box<OpPickerState>,
    },
    EnvRolePicker {
        state: RolePickerState,
    },
    EnvScopePicker {
        state: ScopePickerState,
    },
    EnvConfirm {
        action: SettingsEnvConfirm,
        state: ConfirmState,
    },
    AuthTextInput {
        state: Box<TextInputState>,
    },
    AuthSourcePicker {
        state: SourcePickerState,
    },
    AuthOpPicker {
        state: Box<OpPickerState>,
    },
    AuthSourceFolderPicker {
        state: FileBrowserState,
    },
    AuthForm {
        target: AuthFormTarget,
        state: Box<AuthForm>,
        focus: AuthFormFocus,
        literal_buffer: String,
    },
}

impl<
    EnvValue,
    TextInputState,
    SourcePickerState,
    OpPickerState,
    FileBrowserState,
    MountDstChoiceState,
    RolePickerState,
    ScopePickerState,
    ConfirmState,
    ConfirmSaveState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
> crate::tui::model::ConsoleAnimationTick
    for SettingsModal<
        EnvValue,
        TextInputState,
        SourcePickerState,
        OpPickerState,
        FileBrowserState,
        MountDstChoiceState,
        RolePickerState,
        ScopePickerState,
        ConfirmState,
        ConfirmSaveState,
        AuthFormTarget,
        AuthForm,
        AuthFormFocus,
    >
where
    OpPickerState: crate::tui::model::ConsoleAnimationTick,
{
    fn tick_active_animation(&mut self) -> bool {
        match self {
            Self::EnvOpPicker { state, .. } | Self::AuthOpPicker { state } => {
                state.tick_active_animation()
            }
            Self::MountText { .. }
            | Self::MountFileBrowser { .. }
            | Self::MountDstChoice { .. }
            | Self::MountScopePicker { .. }
            | Self::MountRolePicker { .. }
            | Self::MountConfirm { .. }
            | Self::MountPreviewSave { .. }
            | Self::EnvText { .. }
            | Self::EnvSourcePicker { .. }
            | Self::EnvRolePicker { .. }
            | Self::EnvScopePicker { .. }
            | Self::EnvConfirm { .. }
            | Self::AuthTextInput { .. }
            | Self::AuthSourcePicker { .. }
            | Self::AuthSourceFolderPicker { .. }
            | Self::AuthForm { .. } => false,
        }
    }
}

impl<
    EnvValue,
    TextInputState,
    SourcePickerState,
    OpPickerState,
    FileBrowserState,
    MountDstChoiceState,
    RolePickerState,
    ScopePickerState,
    ConfirmState,
    ConfirmSaveState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
>
    SettingsModal<
        EnvValue,
        TextInputState,
        SourcePickerState,
        OpPickerState,
        FileBrowserState,
        MountDstChoiceState,
        RolePickerState,
        ScopePickerState,
        ConfirmState,
        ConfirmSaveState,
        AuthFormTarget,
        AuthForm,
        AuthFormFocus,
    >
where
    OpPickerState: ModalOpPickerState,
    RolePickerState: ModalRolePickerState,
    ConfirmState: ModalConfirmState,
    ConfirmSaveState: ModalConfirmSaveState,
    AuthForm: ModalAuthFormState,
{
    #[must_use]
    pub fn debug_kind(&self) -> crate::tui::debug::SettingsMountModalDebugKind {
        use crate::tui::debug::SettingsMountModalDebugKind;
        match self {
            Self::MountText { .. } => SettingsMountModalDebugKind::TextInput,
            Self::MountFileBrowser { .. } => SettingsMountModalDebugKind::FileBrowser,
            Self::MountDstChoice { .. } => SettingsMountModalDebugKind::MountDstChoice,
            Self::MountScopePicker { .. } => SettingsMountModalDebugKind::ScopePicker,
            Self::MountRolePicker { .. } => SettingsMountModalDebugKind::RolePicker,
            Self::MountConfirm { action, .. } => match action {
                GlobalMountConfirm::Remove => SettingsMountModalDebugKind::ConfirmRemove,
                GlobalMountConfirm::Save => SettingsMountModalDebugKind::ConfirmSave,
                GlobalMountConfirm::Sensitive => SettingsMountModalDebugKind::ConfirmSensitive,
                GlobalMountConfirm::Discard => SettingsMountModalDebugKind::ConfirmDiscard,
            },
            Self::MountPreviewSave { .. } => SettingsMountModalDebugKind::PreviewSave,
            _ => unreachable!("mount debug facts were requested for a non-mount settings modal"),
        }
    }

    #[must_use]
    pub fn rect_mode(&self) -> ModalRectMode {
        match self {
            Self::MountText { .. } | Self::EnvText { .. } | Self::AuthTextInput { .. } => {
                ModalRectMode::TextInput
            }
            Self::MountFileBrowser { .. } | Self::AuthSourceFolderPicker { .. } => {
                ModalRectMode::FileBrowser
            }
            Self::MountDstChoice { .. } => ModalRectMode::MountChoice,
            Self::MountScopePicker { .. } | Self::EnvScopePicker { .. } => {
                ModalRectMode::ScopePicker
            }
            Self::MountRolePicker { state } | Self::EnvRolePicker { state } => {
                ModalRectMode::RolePicker {
                    filtered_len: state.filtered_len(),
                }
            }
            Self::EnvSourcePicker { .. } | Self::AuthSourcePicker { .. } => {
                ModalRectMode::SourcePicker
            }
            Self::EnvOpPicker { state, .. } | Self::AuthOpPicker { state }
                if state.has_naming_stage_input() =>
            {
                ModalRectMode::TextInput
            }
            Self::EnvOpPicker { .. } | Self::AuthOpPicker { .. } => ModalRectMode::OpPicker,
            Self::MountConfirm { state, .. } | Self::EnvConfirm { state, .. } => {
                ModalRectMode::Confirm {
                    width_pct: state.width_pct(),
                    height: state.required_height(),
                }
            }
            Self::MountPreviewSave { state } => ModalRectMode::ConfirmSave {
                required_height: state.required_height(),
            },
            Self::AuthForm { state, .. } => ModalRectMode::AuthForm {
                required_height: state.required_height(),
            },
        }
    }

    pub fn prepare_for_render(&mut self, outer: ratatui::layout::Rect)
    where
        ConfirmSaveState: ModalConfirmSavePrepareState,
    {
        let modal_area =
            crate::tui::components::modal_rects::modal_rect_for_mode(outer, self.rect_mode());
        if let Self::MountPreviewSave { state } = self {
            state.prepare_for_render(modal_area);
        }
    }

    #[must_use]
    pub fn env_footer_items(&self) -> Vec<jackin_tui::HintSpan<'static>>
    where
        OpPickerState: ModalOpPickerFooterState,
    {
        match self {
            Self::EnvText { .. } => footer_items_for_mode(ModalFooterMode::ConfirmDismiss),
            Self::EnvSourcePicker { .. } | Self::EnvScopePicker { .. } => {
                footer_items_for_mode(ModalFooterMode::SegmentedChoice)
            }
            Self::EnvOpPicker { state, .. } => footer_items_for_mode(state.footer_mode(false)),
            Self::EnvRolePicker { .. } => footer_items_for_mode(ModalFooterMode::FilteredPicker {
                include_refresh: false,
                include_collapse: false,
            }),
            Self::EnvConfirm { .. } => footer_items_for_mode(ModalFooterMode::YesNo),
            _ => Vec::new(),
        }
    }

    #[must_use]
    pub fn mounts_footer_items(&self) -> Vec<jackin_tui::HintSpan<'static>>
    where
        FileBrowserState: ModalFileBrowserFooterState,
        ConfirmSaveState: ModalConfirmSaveFooterState,
    {
        match self {
            Self::MountText { .. } => footer_items_for_mode(ModalFooterMode::ConfirmDismiss),
            Self::MountFileBrowser { state } => state.footer_items(),
            Self::MountDstChoice { .. } => footer_items_for_mode(ModalFooterMode::MountDestination),
            Self::MountScopePicker { .. } => {
                footer_items_for_mode(ModalFooterMode::SegmentedChoice)
            }
            Self::MountRolePicker { .. } => {
                footer_items_for_mode(ModalFooterMode::FilteredPicker {
                    include_refresh: false,
                    include_collapse: false,
                })
            }
            Self::MountConfirm { .. } => footer_items_for_mode(ModalFooterMode::YesNo),
            Self::MountPreviewSave { state } => footer_items_for_mode(state.footer_mode()),
            _ => Vec::new(),
        }
    }

    #[must_use]
    pub fn auth_footer_items(&self, can_generate_token: bool) -> Vec<jackin_tui::HintSpan<'static>>
    where
        FileBrowserState: ModalFileBrowserFooterState,
        OpPickerState: ModalOpPickerFooterState,
        AuthForm: ModalAuthFormFooterState<AuthFormFocus>,
        AuthFormFocus: Copy,
    {
        match self {
            Self::AuthForm { state, focus, .. } => {
                footer_items_for_mode(state.footer_mode(*focus, can_generate_token))
            }
            Self::AuthTextInput { .. } => footer_items_for_mode(ModalFooterMode::ConfirmDismiss),
            Self::AuthSourcePicker { .. } => {
                footer_items_for_mode(ModalFooterMode::SegmentedChoice)
            }
            Self::AuthSourceFolderPicker { state } => state.footer_items(),
            Self::AuthOpPicker { state } => footer_items_for_mode(state.footer_mode(false)),
            _ => Vec::new(),
        }
    }

    #[must_use]
    pub const fn letter_input_kind(&self) -> Option<crate::tui::run::LetterInputModalKind> {
        crate::tui::run::letter_input_modal_kind(
            matches!(self, Self::MountText { .. }),
            false,
            true,
        )
    }
}

impl<
    EnvValue,
    TextInputState,
    SourcePickerState,
    OpPickerState,
    FileBrowserState,
    MountDstChoiceState,
    RolePickerState,
    ScopePickerState,
    ConfirmState,
    ConfirmSaveState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
>
    SettingsModal<
        EnvValue,
        TextInputState,
        SourcePickerState,
        OpPickerState,
        FileBrowserState,
        MountDstChoiceState,
        RolePickerState,
        ScopePickerState,
        ConfirmState,
        ConfirmSaveState,
        AuthFormTarget,
        AuthForm,
        AuthFormFocus,
    >
{
    #[must_use]
    pub const fn mount_scroll_target(&self) -> crate::tui::update::SettingsModalScrollTarget {
        use crate::tui::update::SettingsModalScrollTarget;
        match self {
            Self::MountRolePicker { .. } => SettingsModalScrollTarget::MountRolePicker,
            _ => SettingsModalScrollTarget::None,
        }
    }

    #[must_use]
    pub const fn env_scroll_target(&self) -> crate::tui::update::SettingsModalScrollTarget {
        use crate::tui::update::SettingsModalScrollTarget;
        match self {
            Self::EnvOpPicker { .. } => SettingsModalScrollTarget::EnvOpPicker,
            Self::EnvRolePicker { .. } => SettingsModalScrollTarget::EnvRolePicker,
            _ => SettingsModalScrollTarget::None,
        }
    }

    #[must_use]
    pub const fn auth_scroll_target(&self) -> crate::tui::update::SettingsModalScrollTarget {
        use crate::tui::update::SettingsModalScrollTarget;
        match self {
            Self::AuthOpPicker { .. } => SettingsModalScrollTarget::AuthOpPicker,
            _ => SettingsModalScrollTarget::None,
        }
    }
}

impl<
    EnvValue,
    TextInputState,
    SourcePickerState,
    OpPickerState,
    FileBrowserState,
    MountDstChoiceState,
    RolePickerState,
    ScopePickerState,
    ConfirmState,
    ConfirmSaveState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
> crate::tui::debug::ConsoleSettingsMountModalDebugKind
    for SettingsModal<
        EnvValue,
        TextInputState,
        SourcePickerState,
        OpPickerState,
        FileBrowserState,
        MountDstChoiceState,
        RolePickerState,
        ScopePickerState,
        ConfirmState,
        ConfirmSaveState,
        AuthFormTarget,
        AuthForm,
        AuthFormFocus,
    >
where
    OpPickerState: ModalOpPickerState,
    RolePickerState: ModalRolePickerState,
    ConfirmState: ModalConfirmState,
    ConfirmSaveState: ModalConfirmSaveState,
    AuthForm: ModalAuthFormState,
{
    fn settings_mount_modal_debug_kind(&self) -> crate::tui::debug::SettingsMountModalDebugKind {
        self.debug_kind()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsEnvConfig<V> {
    pub env: BTreeMap<String, V>,
    pub roles: BTreeMap<String, BTreeMap<String, V>>,
}

impl<V> SettingsEnvConfig<V> {
    pub fn map<U>(self, mut f: impl FnMut(V) -> U) -> SettingsEnvConfig<U> {
        SettingsEnvConfig {
            env: self
                .env
                .into_iter()
                .map(|(key, value)| (key, f(value)))
                .collect(),
            roles: self
                .roles
                .into_iter()
                .map(|(role, env)| {
                    (
                        role,
                        env.into_iter()
                            .map(|(key, value)| (key, f(value)))
                            .collect(),
                    )
                })
                .collect(),
        }
    }
}

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalMountConfirm {
    Remove,
    Save,
    Sensitive,
    Discard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMountTextTarget {
    AddScope,
    AddName,
    AddSource,
    AddDestination,
    Source,
    Destination,
    Scope,
    Rename,
}

#[derive(Debug)]
pub struct GlobalMountsState<Row, Modal> {
    pub selected: usize,
    pub pending: Vec<Row>,
    pub original: Vec<Row>,
    pub mount_info_cache: crate::mount_info_cache::MountInfoCache,
    pub modal: Option<Modal>,
    pub modal_parents: Vec<Modal>,
    pub add_draft: Option<GlobalMountDraft>,
    pub error: Option<String>,
    pub scroll_x: u16,
    pub scroll_y: u16,
    /// Dispatcher pops back to the workspace list when set.
    pub exit_requested: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct GlobalMountsSaveRefs<'a, Row> {
    pub original: &'a [Row],
    pub pending: &'a [Row],
}

impl<Row, Modal> GlobalMountsState<Row, Modal> {
    #[must_use]
    pub fn from_rows(rows: Vec<Row>) -> Self
    where
        Row: Clone,
    {
        Self {
            selected: 0,
            pending: rows.clone(),
            original: rows,
            mount_info_cache: crate::mount_info_cache::MountInfoCache::default(),
            modal: None,
            modal_parents: Vec::new(),
            add_draft: None,
            error: None,
            scroll_x: 0,
            scroll_y: 0,
            exit_requested: false,
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool
    where
        Row: PartialEq,
    {
        self.pending != self.original
    }

    #[must_use]
    pub fn save_refs(&self) -> GlobalMountsSaveRefs<'_, Row> {
        GlobalMountsSaveRefs {
            original: &self.original,
            pending: &self.pending,
        }
    }

    pub fn discard(&mut self)
    where
        Row: Clone,
    {
        self.pending = self.original.clone();
        self.mount_info_cache.clear();
        self.selected = self.selected.min(self.pending.len().saturating_sub(1));
        self.add_draft = None;
        let mut stack =
            ModalStack::from_parts(self.modal.take(), std::mem::take(&mut self.modal_parents));
        stack.clear_chain();
        (self.modal, self.modal_parents) = stack.into_parts();
        self.error = None;
    }

    pub fn apply_selection_plan(
        &mut self,
        plan: crate::tui::screens::settings::update::SettingsSelectionScrollPlan,
    ) {
        self.selected = plan.selected;
        self.scroll_y = plan.scroll_y;
    }

    pub fn apply_horizontal_scroll(&mut self, scroll_x: u16) {
        self.scroll_x = scroll_x;
    }

    pub fn mark_saved(&mut self)
    where
        Row: Clone,
    {
        self.original = self.pending.clone();
        self.mount_info_cache.clear();
    }

    pub fn open_sub_modal(&mut self, child: Modal) {
        let mut stack =
            ModalStack::from_parts(self.modal.take(), std::mem::take(&mut self.modal_parents));
        stack.open_sub(child);
        (self.modal, self.modal_parents) = stack.into_parts();
    }

    pub fn start_add_draft(&mut self) {
        self.add_draft = Some(GlobalMountDraft::default());
        let mut stack =
            ModalStack::from_parts(self.modal.take(), std::mem::take(&mut self.modal_parents));
        stack.clear_chain();
        (self.modal, self.modal_parents) = stack.into_parts();
    }

    pub fn remove_row_and_select(&mut self, remove_index: usize, selected: usize) {
        self.pending.remove(remove_index);
        self.selected = selected;
    }

    pub fn pop_modal_chain(&mut self) {
        let mut stack =
            ModalStack::from_parts(self.modal.take(), std::mem::take(&mut self.modal_parents));
        stack.pop();
        (self.modal, self.modal_parents) = stack.into_parts();
    }

    pub fn pop_modal_chain_and_clear_add_draft_if_closed(&mut self) {
        self.pop_modal_chain();
        if self.modal.is_none() {
            self.add_draft = None;
        }
    }

    pub fn clear_modal_chain(&mut self) {
        let mut stack =
            ModalStack::from_parts(self.modal.take(), std::mem::take(&mut self.modal_parents));
        stack.clear_chain();
        (self.modal, self.modal_parents) = stack.into_parts();
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    pub const fn request_exit(&mut self) {
        self.exit_requested = true;
    }

    pub fn take_exit_requested(&mut self) -> bool {
        std::mem::take(&mut self.exit_requested)
    }
}

impl<Row, Modal> SettingsPanelTakeError for GlobalMountsState<Row, Modal> {
    fn take_panel_error(&mut self) -> Option<String> {
        self.take_error()
    }
}

impl<Row, Modal> SettingsMountsTakeExit for GlobalMountsState<Row, Modal> {
    fn take_mounts_exit_requested(&mut self) -> bool {
        self.take_exit_requested()
    }
}

impl<Modal> GlobalMountsState<jackin_config::GlobalMountRow, Modal> {
    #[must_use]
    pub fn content_width(&self) -> usize {
        crate::tui::mount_display::settings_global_config_mounts_content_width_with_cache(
            &self.pending,
            &self.mount_info_cache,
        )
    }

    pub fn add_row_and_close(&mut self, row: jackin_config::GlobalMountRow, selected: usize) {
        self.pending.push(row);
        self.selected = selected;
        self.clear_modal_chain();
    }

    pub fn toggle_selected_readonly(&mut self) {
        if let Some(row) = self.pending.get_mut(self.selected) {
            row.mount.readonly = !row.mount.readonly;
        }
    }
}

impl<Row, Modal> SettingsPanelDirty for GlobalMountsState<Row, Modal>
where
    Row: PartialEq,
{
    fn panel_is_dirty(&self) -> bool {
        self.is_dirty()
    }
}

impl<Row, Modal> SettingsPanelChangeCount for GlobalMountsState<Row, Modal>
where
    Row: PartialEq,
{
    fn panel_change_count(&self) -> usize {
        crate::tui::screens::settings::update::settings_vec_change_count(
            &self.original,
            &self.pending,
        )
    }
}

impl<Row, Modal> SettingsPanelDiscard for GlobalMountsState<Row, Modal>
where
    Row: Clone,
{
    fn panel_discard(&mut self) {
        self.discard();
    }
}

impl<Row, Modal> SettingsPanelMarkSaved for GlobalMountsState<Row, Modal>
where
    Row: Clone,
{
    fn panel_mark_saved(&mut self) {
        self.mark_saved();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsTrustRow {
    pub role: String,
    pub git: String,
    pub trusted: bool,
}

#[derive(Debug)]
pub struct SettingsTrustState {
    pub selected: usize,
    pub pending: Vec<SettingsTrustRow>,
    pub original: Vec<SettingsTrustRow>,
    pub error: Option<String>,
    pub scroll_x: u16,
    pub scroll_y: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct SettingsTrustSaveRefs<'a> {
    pub pending: &'a [SettingsTrustRow],
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GlobalMountDraft {
    pub name: String,
    pub src: String,
    pub dst: String,
    pub scope: Option<String>,
}

fn footer_items_for_mode(mode: ModalFooterMode) -> Vec<jackin_tui::HintSpan<'static>> {
    crate::tui::components::footer_hints::modal_footer_items(mode)
}

#[derive(Debug)]
pub struct SettingsAuthState<EnvValue, Modal, PendingOpCommit> {
    pub selected: usize,
    pub selected_kind: Option<AuthKind>,
    pub pending: Vec<SettingsAuthRow<AuthKind, AuthMode>>,
    pub original: Vec<SettingsAuthRow<AuthKind, AuthMode>>,
    pub github_env: BTreeMap<String, EnvValue>,
    pub original_github_env: BTreeMap<String, EnvValue>,
    pub modal: Option<Modal>,
    /// Parent modal chain for the auth sub-modal stack.
    pub modal_parents: Vec<Modal>,
    /// Set while the `g`/`G` generate action's Create-mode `OpPicker` is open.
    pub generating_token: bool,
    pub error: Option<String>,
    /// In-flight 1Password read for an op-picker auth-form commit.
    pub pending_op_commit: Option<PendingOpCommit>,
    pub scroll_y: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct SettingsAuthSaveRefs<'a, EnvValue> {
    pub pending: &'a [SettingsAuthRow<AuthKind, AuthMode>],
    pub original_github_env: &'a BTreeMap<String, EnvValue>,
    pub github_env: &'a BTreeMap<String, EnvValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub struct SettingsGeneralState {
    pub pending_coauthor_trailer: bool,
    pub original_coauthor_trailer: bool,
    pub pending_dco: bool,
    pub original_dco: bool,
    pub selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsGeneralSaveRefs {
    pub git_coauthor_trailer: bool,
    pub git_dco: bool,
}
