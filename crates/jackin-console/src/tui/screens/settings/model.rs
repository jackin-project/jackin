//! Settings screen state: per-tab state structs for the General, Mounts,
//! Environments, Auth, and Trust panels.
//!
//! Not responsible for: event handling (see `update`) or rendering (see
//! `view`).

use std::collections::BTreeMap;

use crate::tui::auth::{AuthKind, AuthMode};
use jackin_tui::components::FocusOwner;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHoverTarget {
    Tab(usize),
    TrustRow(usize),
}

impl<Mounts, Env, Auth, Trust, ErrorPopup, PendingToken>
    SettingsState<Mounts, Env, Auth, Trust, ErrorPopup, PendingToken>
{
    #[must_use]
    pub const fn focus_owner(&self) -> FocusOwner<SettingsTab> {
        self.focus_owner
    }

    pub fn set_focus_owner(&mut self, owner: FocusOwner<SettingsTab>) {
        self.focus_owner = owner;
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

#[derive(Debug)]
pub enum SettingsEnvModal<
    TextInputState,
    SourcePickerState,
    OpPickerState,
    RolePickerState,
    ScopePickerState,
    ConfirmState,
> {
    Text {
        target: SettingsEnvTextTarget,
        state: Box<TextInputState>,
    },
    SourcePicker {
        state: SourcePickerState,
    },
    OpPicker {
        state: Box<OpPickerState>,
    },
    RolePicker {
        state: RolePickerState,
    },
    ScopePicker {
        state: ScopePickerState,
    },
    Confirm {
        action: SettingsEnvConfirm,
        state: ConfirmState,
    },
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
pub fn settings_env_config_from_app_config(
    config: &jackin_config::AppConfig,
) -> SettingsEnvConfig<jackin_config::EnvValue> {
    SettingsEnvConfig {
        env: config.env.clone(),
        roles: config
            .roles
            .iter()
            .map(|(role, source)| (role.clone(), source.env.clone()))
            .collect(),
    }
}

#[derive(Debug)]
pub struct SettingsEnvState<EnvValue, Modal> {
    pub selected: usize,
    pub pending: SettingsEnvConfig<EnvValue>,
    pub original: SettingsEnvConfig<EnvValue>,
    pub modal: Option<Modal>,
    pub modal_parents: Vec<Modal>,
    pub pending_env_key: Option<(SettingsEnvScope, String)>,
    pub pending_picker_target: Option<(SettingsEnvScope, Option<String>)>,
    pub pending_picker_value: Option<EnvValue>,
    pub unmasked_rows: std::collections::BTreeSet<(SettingsEnvScope, String)>,
    pub expanded: std::collections::BTreeSet<String>,
    pub error: Option<String>,
    pub scroll_y: u16,
}

impl<EnvValue, Modal> SettingsEnvState<EnvValue, Modal> {
    #[must_use]
    pub fn from_config(config: &jackin_config::AppConfig) -> Self
    where
        EnvValue: Clone + From<jackin_config::EnvValue>,
    {
        let pending = settings_env_config_from_app_config(config).map(EnvValue::from);
        Self::from_pending(pending)
    }

    #[must_use]
    pub fn from_pending(pending: SettingsEnvConfig<EnvValue>) -> Self
    where
        EnvValue: Clone,
    {
        Self {
            selected: 0,
            original: pending.clone(),
            pending,
            modal: None,
            modal_parents: Vec::new(),
            pending_env_key: None,
            pending_picker_target: None,
            pending_picker_value: None,
            unmasked_rows: std::collections::BTreeSet::default(),
            expanded: std::collections::BTreeSet::default(),
            error: None,
            scroll_y: 0,
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool
    where
        EnvValue: PartialEq,
    {
        self.pending != self.original
    }

    pub fn discard(&mut self)
    where
        EnvValue: Clone,
    {
        self.pending = self.original.clone();
        self.selected = self.selected.min(
            crate::tui::screens::settings::update::settings_env_flat_row_count(
                &self.pending,
                &self.expanded,
            )
            .saturating_sub(1),
        );
        self.modal = None;
        self.modal_parents.clear();

        self.pending_picker_target = None;
        self.pending_picker_value = None;
        self.unmasked_rows.clear();
        self.expanded.clear();
        self.error = None;
    }

    #[must_use]
    pub fn change_count(&self) -> usize
    where
        EnvValue: PartialEq,
    {
        crate::tui::screens::settings::update::settings_map_change_count(
            &self.original.env,
            &self.pending.env,
        ) + self
            .original
            .roles
            .keys()
            .chain(self.pending.roles.keys())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|role| {
                let empty = BTreeMap::new();
                let original = self.original.roles.get(role).unwrap_or(&empty);
                let pending = self.pending.roles.get(role).unwrap_or(&empty);
                crate::tui::screens::settings::update::settings_map_change_count(original, pending)
            })
            .sum::<usize>()
    }

    pub fn open_sub_modal(&mut self, child: Modal) {
        if let Some(parent) = self.modal.take() {
            self.modal_parents.push(parent);
        }
        self.modal = Some(child);
    }

    pub fn pop_modal_chain(&mut self) {
        self.modal = self.modal_parents.pop();
        if self.modal.is_none() {
            self.drop_modal_scratch();
        }
    }

    pub fn clear_modal_chain(&mut self) {
        self.modal = None;
        self.modal_parents.clear();
        self.drop_modal_scratch();
    }

    fn drop_modal_scratch(&mut self) {
        self.pending_picker_value = None;
    }

    pub fn mark_saved(&mut self)
    where
        EnvValue: Clone,
    {
        self.original = self.pending.clone();
    }
}

impl<EnvValue, Modal> SettingsPanelDirty for SettingsEnvState<EnvValue, Modal>
where
    EnvValue: PartialEq,
{
    fn panel_is_dirty(&self) -> bool {
        self.is_dirty()
    }
}

impl<EnvValue, Modal> SettingsPanelChangeCount for SettingsEnvState<EnvValue, Modal>
where
    EnvValue: PartialEq,
{
    fn panel_change_count(&self) -> usize {
        self.change_count()
    }
}

impl<EnvValue, Modal> SettingsPanelDiscard for SettingsEnvState<EnvValue, Modal>
where
    EnvValue: Clone,
{
    fn panel_discard(&mut self) {
        self.discard();
    }
}

impl<EnvValue, Modal> SettingsPanelMarkSaved for SettingsEnvState<EnvValue, Modal>
where
    EnvValue: Clone,
{
    fn panel_mark_saved(&mut self) {
        self.mark_saved();
    }
}

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
pub enum GlobalMountModal<
    TextInputState,
    FileBrowserState,
    MountDstChoiceState,
    ScopePickerState,
    RolePickerState,
    ConfirmState,
    ConfirmSaveState,
> {
    Text {
        target: GlobalMountTextTarget,
        state: Box<TextInputState>,
    },
    FileBrowser {
        state: Box<FileBrowserState>,
    },
    MountDstChoice {
        state: MountDstChoiceState,
    },
    ScopePicker {
        state: ScopePickerState,
    },
    RolePicker {
        state: RolePickerState,
    },
    Confirm {
        action: GlobalMountConfirm,
        state: ConfirmState,
    },
    PreviewSave {
        state: ConfirmSaveState,
    },
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

    pub fn discard(&mut self)
    where
        Row: Clone,
    {
        self.pending = self.original.clone();
        self.mount_info_cache.clear();
        self.selected = self.selected.min(self.pending.len().saturating_sub(1));
        self.add_draft = None;
        self.modal = None;
        self.modal_parents.clear();
        self.error = None;
    }

    pub fn mark_saved(&mut self)
    where
        Row: Clone,
    {
        self.original = self.pending.clone();
        self.mount_info_cache.clear();
    }

    pub fn open_sub_modal(&mut self, child: Modal) {
        if let Some(parent) = self.modal.take() {
            self.modal_parents.push(parent);
        }
        self.modal = Some(child);
    }

    pub fn pop_modal_chain(&mut self) {
        self.modal = self.modal_parents.pop();
    }

    pub fn clear_modal_chain(&mut self) {
        self.modal = None;
        self.modal_parents.clear();
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

impl SettingsTrustState {
    #[must_use]
    pub fn from_config(config: &jackin_config::AppConfig) -> Self {
        Self::from_rows(settings_trust_rows_from_app_config(config))
    }

    #[must_use]
    pub fn from_rows(pending: Vec<SettingsTrustRow>) -> Self {
        Self {
            selected: 0,
            original: pending.clone(),
            pending,
            error: None,
            scroll_x: 0,
            scroll_y: 0,
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.pending != self.original
    }

    pub fn discard(&mut self) {
        self.pending = self.original.clone();
        self.selected = self.selected.min(self.pending.len().saturating_sub(1));
        self.error = None;
    }

    pub fn mark_saved(&mut self) {
        self.original = self.pending.clone();
    }
}

impl SettingsPanelDirty for SettingsTrustState {
    fn panel_is_dirty(&self) -> bool {
        self.is_dirty()
    }
}

impl SettingsPanelChangeCount for SettingsTrustState {
    fn panel_change_count(&self) -> usize {
        crate::tui::screens::settings::update::settings_vec_change_count(
            &self.original,
            &self.pending,
        )
    }
}

impl SettingsPanelDiscard for SettingsTrustState {
    fn panel_discard(&mut self) {
        self.discard();
    }
}

impl SettingsPanelMarkSaved for SettingsTrustState {
    fn panel_mark_saved(&mut self) {
        self.mark_saved();
    }
}

#[must_use]
pub fn settings_trust_rows_from_app_config(
    config: &jackin_config::AppConfig,
) -> Vec<SettingsTrustRow> {
    config
        .roles
        .iter()
        .map(|(role, source)| SettingsTrustRow {
            role: role.clone(),
            git: source.git.clone(),
            trusted: source.trusted,
        })
        .collect()
}

#[derive(Debug, Default)]
pub struct GlobalMountDraft {
    pub name: String,
    pub src: String,
    pub dst: String,
    pub scope: Option<String>,
}

#[derive(Debug)]
pub enum SettingsAuthModal<
    TextInputState,
    SourcePickerState,
    OpPickerState,
    FileBrowserState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
> {
    TextInput {
        state: Box<TextInputState>,
    },
    SourcePicker {
        state: SourcePickerState,
    },
    OpPicker {
        state: Box<OpPickerState>,
    },
    SourceFolderPicker {
        state: FileBrowserState,
    },
    AuthForm {
        target: AuthFormTarget,
        state: Box<AuthForm>,
        focus: AuthFormFocus,
        literal_buffer: String,
    },
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

impl<EnvValue, Modal, PendingOpCommit> SettingsAuthState<EnvValue, Modal, PendingOpCommit> {
    #[must_use]
    pub fn from_config(config: &jackin_config::AppConfig) -> Self
    where
        EnvValue: Clone + From<jackin_config::EnvValue>,
    {
        let github_env = crate::tui::auth_config::app_github_env(config)
            .into_iter()
            .map(|(key, value)| (key, EnvValue::from(value)))
            .collect();
        let pending = crate::tui::auth_config::settings_auth_rows_from_app_config(config);
        Self::from_rows_and_github_env(pending, github_env)
    }

    #[must_use]
    pub fn from_rows_and_github_env(
        pending: Vec<SettingsAuthRow<AuthKind, AuthMode>>,
        github_env: BTreeMap<String, EnvValue>,
    ) -> Self
    where
        EnvValue: Clone,
    {
        Self {
            selected: 0,
            selected_kind: None,
            original: pending.clone(),
            pending,
            github_env: github_env.clone(),
            original_github_env: github_env,
            modal: None,
            modal_parents: Vec::new(),
            generating_token: false,
            error: None,
            pending_op_commit: None,
            scroll_y: 0,
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool
    where
        EnvValue: PartialEq,
    {
        self.pending != self.original || self.github_env != self.original_github_env
    }

    #[must_use]
    pub fn row_count(&self) -> usize {
        let Some(kind) = self.selected_kind else {
            return self.pending.len();
        };
        let Some(row) = self.pending.iter().find(|row| row.kind == kind) else {
            return 0;
        };
        crate::tui::screens::settings::update::settings_auth_detail_row_count(kind, row.mode)
    }

    pub fn discard(&mut self)
    where
        EnvValue: Clone,
    {
        self.pending = self.original.clone();
        self.github_env = self.original_github_env.clone();
        self.selected_kind = None;
        self.selected = self.selected.min(self.pending.len().saturating_sub(1));
        self.modal = None;
        self.modal_parents.clear();
        self.generating_token = false;
        self.error = None;
    }

    pub fn mark_saved(&mut self)
    where
        EnvValue: Clone,
    {
        self.original = self.pending.clone();
        self.original_github_env = self.github_env.clone();
    }

    pub fn restore_pending_auth_form(&mut self) {
        self.modal = self.modal_parents.pop();
    }

    /// Push the current auth modal onto the parent stack so a sub-modal can
    /// open without losing the auth form's in-progress state.
    pub fn push_auth_modal(&mut self, sub_modal: Modal) {
        if let Some(current) = self.modal.take() {
            self.modal_parents.push(current);
        }
        self.modal = Some(sub_modal);
    }
}

impl<EnvValue, Modal, PendingOpCommit> SettingsPanelDirty
    for SettingsAuthState<EnvValue, Modal, PendingOpCommit>
where
    EnvValue: PartialEq,
{
    fn panel_is_dirty(&self) -> bool {
        self.is_dirty()
    }
}

impl<EnvValue, Modal, PendingOpCommit> SettingsPanelChangeCount
    for SettingsAuthState<EnvValue, Modal, PendingOpCommit>
where
    EnvValue: PartialEq,
{
    fn panel_change_count(&self) -> usize {
        crate::tui::screens::settings::update::settings_vec_change_count(
            &self.original,
            &self.pending,
        ) + crate::tui::screens::settings::update::settings_map_change_count(
            &self.original_github_env,
            &self.github_env,
        )
    }
}

impl<EnvValue, Modal, PendingOpCommit> SettingsPanelDiscard
    for SettingsAuthState<EnvValue, Modal, PendingOpCommit>
where
    EnvValue: Clone,
{
    fn panel_discard(&mut self) {
        self.discard();
    }
}

impl<EnvValue, Modal, PendingOpCommit> SettingsPanelMarkSaved
    for SettingsAuthState<EnvValue, Modal, PendingOpCommit>
where
    EnvValue: Clone,
{
    fn panel_mark_saved(&mut self) {
        self.mark_saved();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct SettingsGeneralState {
    pub pending_coauthor_trailer: bool,
    pub original_coauthor_trailer: bool,
    pub pending_dco: bool,
    pub original_dco: bool,
    pub selected: usize,
}

impl SettingsGeneralState {
    #[must_use]
    pub const fn from_values(coauthor_trailer: bool, dco: bool) -> Self {
        Self {
            pending_coauthor_trailer: coauthor_trailer,
            original_coauthor_trailer: coauthor_trailer,
            pending_dco: dco,
            original_dco: dco,
            selected: 0,
        }
    }

    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.pending_coauthor_trailer != self.original_coauthor_trailer
            || self.pending_dco != self.original_dco
    }

    pub const fn discard(&mut self) {
        self.pending_coauthor_trailer = self.original_coauthor_trailer;
        self.pending_dco = self.original_dco;
    }

    #[must_use]
    pub fn change_count(&self) -> usize {
        usize::from(self.pending_coauthor_trailer != self.original_coauthor_trailer)
            + usize::from(self.pending_dco != self.original_dco)
    }

    pub const fn mark_clean(&mut self) {
        self.original_coauthor_trailer = self.pending_coauthor_trailer;
        self.original_dco = self.pending_dco;
    }
}

impl SettingsPanelDirty for SettingsGeneralState {
    fn panel_is_dirty(&self) -> bool {
        self.is_dirty()
    }
}

impl SettingsPanelChangeCount for SettingsGeneralState {
    fn panel_change_count(&self) -> usize {
        self.change_count()
    }
}

impl SettingsPanelDiscard for SettingsGeneralState {
    fn panel_discard(&mut self) {
        self.discard();
    }
}

impl SettingsPanelMarkSaved for SettingsGeneralState {
    fn panel_mark_saved(&mut self) {
        self.mark_clean();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use jackin_config::{
        AgentAuthConfig, AppConfig, AuthForwardMode, EnvValue, GlobalMountRow, MountConfig,
        RoleSource,
    };
    use jackin_tui::components::FocusOwner;

    use super::{
        GlobalMountsState, SettingsAuthRow, SettingsAuthState, SettingsEnvConfig, SettingsEnvRow,
        SettingsEnvScope, SettingsEnvState, SettingsGeneralState, SettingsState, SettingsTrustRow,
        SettingsTrustState, settings_env_config_from_app_config,
        settings_trust_rows_from_app_config,
    };

    #[test]
    fn settings_env_config_from_app_config_copies_global_and_role_env() {
        let mut config = AppConfig::default();
        config
            .env
            .insert("GLOBAL".into(), EnvValue::Plain("1".into()));
        config.roles.insert(
            "alpha".into(),
            RoleSource {
                git: "https://example.invalid/alpha.git".into(),
                trusted: true,
                env: BTreeMap::from([("ROLE".into(), EnvValue::Plain("2".into()))]),
            },
        );

        let out = settings_env_config_from_app_config(&config);

        assert_eq!(out.env.get("GLOBAL"), Some(&EnvValue::Plain("1".into())));
        assert_eq!(
            out.roles.get("alpha").and_then(|role| role.get("ROLE")),
            Some(&EnvValue::Plain("2".into()))
        );
    }

    #[test]
    fn settings_env_state_from_config_sets_original_and_pending() {
        let mut config = AppConfig::default();
        config.env.insert("KEY".into(), EnvValue::Plain("1".into()));

        let state = SettingsEnvState::<EnvValue, ()>::from_config(&config);

        assert_eq!(
            state.pending.env.get("KEY"),
            Some(&EnvValue::Plain("1".into()))
        );
        assert_eq!(state.original, state.pending);
        assert!(state.modal.is_none());
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn settings_trust_rows_from_app_config_copies_role_trust_facts() {
        let mut config = AppConfig::default();
        config.roles.insert(
            "alpha".into(),
            RoleSource {
                git: "https://example.invalid/alpha.git".into(),
                trusted: true,
                env: BTreeMap::new(),
            },
        );

        let rows = settings_trust_rows_from_app_config(&config);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].role, "alpha");
        assert_eq!(rows[0].git, "https://example.invalid/alpha.git");
        assert!(rows[0].trusted);
    }

    #[test]
    fn settings_trust_state_from_config_sets_original_and_pending() {
        let mut config = AppConfig::default();
        config.roles.insert(
            "alpha".into(),
            RoleSource {
                git: "https://example.invalid/alpha.git".into(),
                trusted: true,
                env: BTreeMap::new(),
            },
        );

        let state = SettingsTrustState::from_config(&config);

        assert_eq!(state.pending, state.original);
        assert_eq!(state.pending[0].role, "alpha");
        assert!(state.error.is_none());
    }

    #[test]
    fn global_mounts_state_from_rows_sets_original_and_pending() {
        let state = GlobalMountsState::<String, ()>::from_rows(vec!["one".into()]);

        assert_eq!(state.selected, 0);
        assert_eq!(state.pending, vec![String::from("one")]);
        assert_eq!(state.original, vec![String::from("one")]);
        assert!(state.modal.is_none());
        assert!(!state.exit_requested);
    }

    #[test]
    fn settings_auth_state_from_rows_and_github_env_sets_originals() {
        let rows = vec![SettingsAuthRow {
            kind: crate::tui::auth::AuthKind::Github,
            mode: crate::tui::auth::AuthMode::Token,
            sync_source_dir: None,
        }];
        let github_env = BTreeMap::from([("GH_TOKEN".into(), EnvValue::Plain("token".into()))]);

        let state = SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(
            rows.clone(),
            github_env.clone(),
        );

        assert_eq!(state.selected, 0);
        assert_eq!(state.pending, rows);
        let rows = state.pending.clone();
        assert_eq!(state.original, rows);
        assert_eq!(state.github_env, github_env);
        assert_eq!(state.original_github_env, github_env);
        assert!(state.modal.is_none());
    }

    #[test]
    fn settings_auth_state_from_config_sets_rows_and_originals() {
        let config = AppConfig {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::ApiKey,
                ..Default::default()
            }),
            ..Default::default()
        };

        let state = SettingsAuthState::<EnvValue, (), ()>::from_config(&config);

        assert_eq!(state.pending, state.original);
        assert_eq!(state.github_env, state.original_github_env);
        assert!(
            state
                .pending
                .iter()
                .any(|row| row.kind == crate::tui::auth::AuthKind::Claude
                    && row.mode == crate::tui::auth::AuthMode::ApiKey)
        );
    }

    #[test]
    fn settings_state_clears_ignored_env_only_auth_keys() {
        let env: SettingsEnvState<EnvValue, ()> = SettingsEnvState {
            selected: 0,
            pending: SettingsEnvConfig {
                env: BTreeMap::from([(
                    jackin_core::env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
                    EnvValue::Plain("zai".into()),
                )]),
                roles: BTreeMap::new(),
            },
            original: SettingsEnvConfig {
                env: BTreeMap::new(),
                roles: BTreeMap::new(),
            },
            modal: None,
            modal_parents: Vec::new(),
            pending_env_key: None,
            pending_picker_target: None,
            pending_picker_value: None,
            unmasked_rows: std::collections::BTreeSet::default(),
            expanded: std::collections::BTreeSet::default(),
            error: None,
            scroll_y: 0,
        };
        let mut state = SettingsState {
            active_tab: super::SettingsTab::General,
            focus_owner: FocusOwner::TabBar,
            hover_target: None,
            general: SettingsGeneralState::from_values(false, false),
            mounts: GlobalMountsState::<String, ()>::from_rows(Vec::new()),
            env,
            auth: SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(
                vec![SettingsAuthRow {
                    kind: crate::tui::auth::AuthKind::Zai,
                    mode: crate::tui::auth::AuthMode::Ignore,
                    sync_source_dir: None,
                }],
                BTreeMap::new(),
            ),
            trust: SettingsTrustState::from_rows(Vec::new()),
            error_popup: None::<()>,
            pending_token_generate: None::<()>,
            cached_footer_h: 1,
        };

        state.clear_ignored_env_only_auth_keys();

        assert!(
            !state
                .env
                .pending
                .env
                .contains_key(jackin_core::env_model::ZAI_API_KEY_ENV_NAME)
        );
    }

    #[test]
    fn settings_state_from_config_builds_all_panels_clean() {
        let mut config = AppConfig::default();
        config.git.dco = true;
        config.env.insert("KEY".into(), EnvValue::Plain("1".into()));

        type TestState = SettingsState<
            GlobalMountsState<GlobalMountRow, ()>,
            SettingsEnvState<EnvValue, ()>,
            SettingsAuthState<EnvValue, (), ()>,
            SettingsTrustState,
            (),
            (),
        >;

        let state = TestState::from_config(&config);

        assert!(state.general.pending_dco);
        assert_eq!(
            state.env.pending.env.get("KEY"),
            Some(&EnvValue::Plain("1".into()))
        );
        assert!(!state.is_dirty());
        assert_eq!(state.change_count(), 0);
    }

    #[test]
    fn settings_state_env_flat_rows_reads_pending_env() {
        type TestState = SettingsState<
            GlobalMountsState<GlobalMountRow, ()>,
            SettingsEnvState<EnvValue, ()>,
            SettingsAuthState<EnvValue, (), ()>,
            SettingsTrustState,
            (),
            (),
        >;
        let mut state = TestState::from_config(&AppConfig::default());
        state
            .env
            .pending
            .env
            .insert("KEY".into(), EnvValue::Plain("1".into()));

        assert!(state.env_flat_rows().iter().any(|row| matches!(
            row,
            SettingsEnvRow::Key {
                scope: SettingsEnvScope::Global,
                key
            } if key == "KEY"
        )));
    }

    #[test]
    fn settings_state_owns_settings_geometry_facts() {
        type TestState = SettingsState<
            GlobalMountsState<GlobalMountRow, ()>,
            SettingsEnvState<EnvValue, ()>,
            SettingsAuthState<EnvValue, (), ()>,
            SettingsTrustState,
            (),
            (),
        >;
        let mut state = TestState::from_config(&AppConfig::default());
        state.mounts.pending.push(GlobalMountRow {
            scope: None,
            name: "cache".into(),
            mount: MountConfig {
                src: "/tmp/cache".into(),
                dst: "/home/agent/.cache".into(),
                readonly: false,
                isolation: jackin_core::isolation::MountIsolation::Shared,
            },
        });
        state
            .env
            .pending
            .env
            .insert("KEY".into(), EnvValue::Plain("1".into()));
        state.trust.pending = vec![SettingsTrustRow {
            role: "smith".into(),
            git: "https://example.invalid/smith.git".into(),
            trusted: false,
        }];
        state.mounts.error = Some("bad mount".into());
        state.env.error = Some("bad env".into());
        state.auth.error = Some("bad auth".into());
        state.trust.error = Some("bad trust".into());
        state.mounts.scroll_x = 1000;

        assert!(state.mounts_content_height() >= 2);
        assert!(state.env_content_height() >= 3);
        assert!(state.auth_content_height() >= 2);
        assert!(state.trust_content_height() >= 3);

        state.clamp_mounts_scroll_for_frame(ratatui::layout::Rect::new(0, 0, 120, 30));

        assert!(state.mounts.scroll_x < 1000);
    }
}
