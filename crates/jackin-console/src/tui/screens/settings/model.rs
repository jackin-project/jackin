//! Settings screen state: per-tab state structs for the General, Mounts,
//! Environments, Auth, and Trust panels.
//!
//! Not responsible for: event handling (see `update`) or rendering (see
//! `view`).

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

pub trait SettingsAuthModalSlot {
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
    Auth: SettingsAuthModalSlot,
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

impl<
    TextInputState,
    SourcePickerState,
    OpPickerState,
    RolePickerState,
    ScopePickerState,
    ConfirmState,
> crate::tui::model::ConsoleAnimationTick
    for SettingsEnvModal<
        TextInputState,
        SourcePickerState,
        OpPickerState,
        RolePickerState,
        ScopePickerState,
        ConfirmState,
    >
where
    OpPickerState: crate::tui::model::ConsoleAnimationTick,
{
    fn tick_active_animation(&mut self) -> bool {
        match self {
            Self::OpPicker { state } => state.tick_active_animation(),
            Self::Text { .. }
            | Self::SourcePicker { .. }
            | Self::RolePicker { .. }
            | Self::ScopePicker { .. }
            | Self::Confirm { .. } => false,
        }
    }
}

impl<
    TextInputState,
    SourcePickerState,
    OpPickerState,
    RolePickerState,
    ScopePickerState,
    ConfirmState,
>
    SettingsEnvModal<
        TextInputState,
        SourcePickerState,
        OpPickerState,
        RolePickerState,
        ScopePickerState,
        ConfirmState,
    >
where
    OpPickerState: ModalOpPickerState,
    RolePickerState: ModalRolePickerState,
    ConfirmState: ModalConfirmState,
{
    #[must_use]
    pub fn rect_mode(&self) -> ModalRectMode {
        match self {
            Self::Text { .. } => ModalRectMode::TextInput,
            Self::SourcePicker { .. } => ModalRectMode::SourcePicker,
            Self::OpPicker { state } if state.has_naming_stage_input() => ModalRectMode::TextInput,
            Self::OpPicker { .. } => ModalRectMode::OpPicker,
            Self::RolePicker { state } => ModalRectMode::RolePicker {
                filtered_len: state.filtered_len(),
            },
            Self::ScopePicker { .. } => ModalRectMode::ScopePicker,
            Self::Confirm { state, .. } => ModalRectMode::Confirm {
                width_pct: state.width_pct(),
                height: state.required_height(),
            },
        }
    }

    #[must_use]
    pub const fn scroll_target(&self) -> crate::tui::update::SettingsEnvModalScrollTarget {
        use crate::tui::update::SettingsEnvModalScrollTarget;
        match self {
            Self::OpPicker { .. } => SettingsEnvModalScrollTarget::OpPicker,
            Self::RolePicker { .. } => SettingsEnvModalScrollTarget::RolePicker,
            _ => SettingsEnvModalScrollTarget::None,
        }
    }

    #[must_use]
    pub fn footer_items(&self) -> Vec<jackin_tui::HintSpan<'static>>
    where
        OpPickerState: ModalOpPickerFooterState,
    {
        match self {
            Self::Text { .. } => footer_items_for_mode(ModalFooterMode::ConfirmDismiss),
            Self::SourcePicker { .. } | Self::ScopePicker { .. } => {
                footer_items_for_mode(ModalFooterMode::SegmentedChoice)
            }
            Self::OpPicker { state } => footer_items_for_mode(state.footer_mode(false)),
            Self::RolePicker { .. } => footer_items_for_mode(ModalFooterMode::FilteredPicker {
                include_refresh: false,
                include_collapse: false,
            }),
            Self::Confirm { .. } => footer_items_for_mode(ModalFooterMode::YesNo),
        }
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

#[derive(Debug, Clone, Copy)]
pub struct SettingsEnvSaveRefs<'a, EnvValue> {
    pub original: &'a SettingsEnvConfig<EnvValue>,
    pub pending: &'a SettingsEnvConfig<EnvValue>,
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

    #[must_use]
    pub const fn save_refs(&self) -> SettingsEnvSaveRefs<'_, EnvValue> {
        SettingsEnvSaveRefs {
            original: &self.original,
            pending: &self.pending,
        }
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

    pub fn apply_selection_plan(
        &mut self,
        plan: crate::tui::screens::settings::update::SettingsSelectionScrollPlan,
    ) {
        self.selected = plan.selected;
        self.scroll_y = plan.scroll_y;
    }

    pub fn set_role_expanded(&mut self, role: String, expanded: bool) {
        if expanded {
            self.expanded.insert(role);
        } else {
            self.expanded.remove(&role);
        }
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

    pub fn pop_modal_chain_and_clear_pending_env_key_if_closed(&mut self) {
        self.pop_modal_chain();
        if self.modal.is_none() {
            self.pending_env_key = None;
        }
    }

    pub fn pop_modal_chain_and_clear_pending_env_key(&mut self) {
        self.pop_modal_chain();
        self.pending_env_key = None;
        self.pending_picker_value = None;
    }

    pub fn pop_modal_chain_and_clear_picker_target(&mut self) {
        self.pop_modal_chain();
        self.pending_picker_target = None;
        self.pending_picker_value = None;
    }

    pub fn set_pending_picker_target(&mut self, target: (SettingsEnvScope, Option<String>)) {
        self.pending_picker_target = Some(target);
    }

    pub fn set_pending_env_key(&mut self, scope: SettingsEnvScope, key: String) {
        self.pending_env_key = Some((scope, key));
    }

    pub fn clear_pending_env_key(&mut self) {
        self.pending_env_key = None;
    }

    pub fn clear_pending_picker_target(&mut self) {
        self.pending_picker_target = None;
    }

    pub fn stash_pending_picker_value(&mut self, value: EnvValue) {
        self.pending_picker_value = Some(value);
    }

    #[must_use]
    pub fn has_pending_picker_value(&self) -> bool {
        self.pending_picker_value.is_some()
    }

    pub fn take_pending_picker_value(&mut self) -> Option<EnvValue> {
        self.pending_picker_value.take()
    }

    pub fn set_value(&mut self, scope: &SettingsEnvScope, key: &str, value: EnvValue) {
        crate::tui::screens::settings::update::set_settings_env_value(
            &mut self.pending,
            &mut self.expanded,
            scope,
            key,
            value,
        );
    }

    pub fn expand_role(&mut self, role: String) {
        self.expanded.insert(role);
    }

    pub fn remove_selected_row(&mut self) -> bool {
        crate::tui::screens::settings::update::remove_selected_settings_env_row(
            &mut self.pending,
            &self.expanded,
            &mut self.selected,
        )
    }

    pub fn clear_modal_chain(&mut self) {
        self.modal = None;
        self.modal_parents.clear();
        self.drop_modal_scratch();
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
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

    #[must_use]
    pub fn pending_value(&self, scope: &SettingsEnvScope, key: &str) -> Option<&EnvValue> {
        crate::tui::screens::settings::update::settings_env_value(&self.pending, scope, key)
    }

    #[must_use]
    pub fn is_unmasked(&self, scope: &SettingsEnvScope, key: &str) -> bool {
        self.unmasked_rows
            .contains(&(scope.clone(), key.to_owned()))
    }
}

impl<EnvValue, Modal> SettingsModalSlot for SettingsEnvState<EnvValue, Modal> {
    type Modal = Modal;

    fn modal_mut(&mut self) -> Option<&mut Self::Modal> {
        self.modal.as_mut()
    }
}

impl<EnvValue, Modal> SettingsPanelTakeError for SettingsEnvState<EnvValue, Modal> {
    fn take_panel_error(&mut self) -> Option<String> {
        self.take_error()
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

impl<
    TextInputState,
    FileBrowserState,
    MountDstChoiceState,
    ScopePickerState,
    RolePickerState,
    ConfirmState,
    ConfirmSaveState,
>
    GlobalMountModal<
        TextInputState,
        FileBrowserState,
        MountDstChoiceState,
        ScopePickerState,
        RolePickerState,
        ConfirmState,
        ConfirmSaveState,
    >
{
    #[must_use]
    pub const fn debug_kind(&self) -> crate::tui::debug::SettingsMountModalDebugKind {
        use crate::tui::debug::SettingsMountModalDebugKind;
        match self {
            Self::Text { .. } => SettingsMountModalDebugKind::TextInput,
            Self::FileBrowser { .. } => SettingsMountModalDebugKind::FileBrowser,
            Self::MountDstChoice { .. } => SettingsMountModalDebugKind::MountDstChoice,
            Self::ScopePicker { .. } => SettingsMountModalDebugKind::ScopePicker,
            Self::RolePicker { .. } => SettingsMountModalDebugKind::RolePicker,
            Self::Confirm { action, .. } => match action {
                GlobalMountConfirm::Remove => SettingsMountModalDebugKind::ConfirmRemove,
                GlobalMountConfirm::Save => SettingsMountModalDebugKind::ConfirmSave,
                GlobalMountConfirm::Sensitive => SettingsMountModalDebugKind::ConfirmSensitive,
                GlobalMountConfirm::Discard => SettingsMountModalDebugKind::ConfirmDiscard,
            },
            Self::PreviewSave { .. } => SettingsMountModalDebugKind::PreviewSave,
        }
    }

    #[must_use]
    pub const fn scroll_target(&self) -> crate::tui::update::GlobalMountModalScrollTarget {
        use crate::tui::update::GlobalMountModalScrollTarget;
        match self {
            Self::RolePicker { .. } => GlobalMountModalScrollTarget::RolePicker,
            _ => GlobalMountModalScrollTarget::None,
        }
    }

    #[must_use]
    pub const fn letter_input_kind(&self) -> Option<crate::tui::run::LetterInputModalKind> {
        crate::tui::run::letter_input_modal_kind(matches!(self, Self::Text { .. }), false, true)
    }

    #[must_use]
    pub fn rect_mode(&self) -> ModalRectMode
    where
        RolePickerState: ModalRolePickerState,
        ConfirmState: ModalConfirmState,
        ConfirmSaveState: ModalConfirmSaveState,
    {
        match self {
            Self::Text { .. } => ModalRectMode::TextInput,
            Self::FileBrowser { .. } => ModalRectMode::FileBrowser,
            Self::MountDstChoice { .. } => ModalRectMode::MountChoice,
            Self::ScopePicker { .. } => ModalRectMode::ScopePicker,
            Self::RolePicker { state } => ModalRectMode::RolePicker {
                filtered_len: state.filtered_len(),
            },
            Self::Confirm { state, .. } => ModalRectMode::Confirm {
                width_pct: state.width_pct(),
                height: state.required_height(),
            },
            Self::PreviewSave { state } => ModalRectMode::ConfirmSave {
                required_height: state.required_height(),
            },
        }
    }

    pub fn prepare_for_render(&mut self, outer: ratatui::layout::Rect)
    where
        RolePickerState: ModalRolePickerState,
        ConfirmState: ModalConfirmState,
        ConfirmSaveState: ModalConfirmSaveState + ModalConfirmSavePrepareState,
    {
        let modal_area =
            crate::tui::components::modal_rects::modal_rect_for_mode(outer, self.rect_mode());
        if let Self::PreviewSave { state } = self {
            state.prepare_for_render(modal_area);
        }
    }

    #[must_use]
    pub fn footer_items(&self) -> Vec<jackin_tui::HintSpan<'static>>
    where
        FileBrowserState: ModalFileBrowserFooterState,
        ConfirmSaveState: ModalConfirmSaveFooterState,
    {
        match self {
            Self::Text { .. } => footer_items_for_mode(ModalFooterMode::ConfirmDismiss),
            Self::FileBrowser { state } => state.footer_items(),
            Self::MountDstChoice { .. } => footer_items_for_mode(ModalFooterMode::MountDestination),
            Self::ScopePicker { .. } => footer_items_for_mode(ModalFooterMode::SegmentedChoice),
            Self::RolePicker { .. } => footer_items_for_mode(ModalFooterMode::FilteredPicker {
                include_refresh: false,
                include_collapse: false,
            }),
            Self::Confirm { .. } => footer_items_for_mode(ModalFooterMode::YesNo),
            Self::PreviewSave { state } => footer_items_for_mode(state.footer_mode()),
        }
    }
}

impl<
    TextInputState,
    FileBrowserState,
    MountDstChoiceState,
    ScopePickerState,
    RolePickerState,
    ConfirmState,
    ConfirmSaveState,
> crate::tui::debug::ConsoleSettingsMountModalDebugKind
    for GlobalMountModal<
        TextInputState,
        FileBrowserState,
        MountDstChoiceState,
        ScopePickerState,
        RolePickerState,
        ConfirmState,
        ConfirmSaveState,
    >
{
    fn settings_mount_modal_debug_kind(&self) -> crate::tui::debug::SettingsMountModalDebugKind {
        self.debug_kind()
    }
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
        self.modal = None;
        self.modal_parents.clear();
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
        if let Some(parent) = self.modal.take() {
            self.modal_parents.push(parent);
        }
        self.modal = Some(child);
    }

    pub fn start_add_draft(&mut self) {
        self.add_draft = Some(GlobalMountDraft::default());
        self.modal_parents.clear();
    }

    pub fn remove_row_and_select(&mut self, remove_index: usize, selected: usize) {
        self.pending.remove(remove_index);
        self.selected = selected;
    }

    pub fn pop_modal_chain(&mut self) {
        self.modal = self.modal_parents.pop();
    }

    pub fn pop_modal_chain_and_clear_add_draft_if_closed(&mut self) {
        self.pop_modal_chain();
        if self.modal.is_none() {
            self.add_draft = None;
        }
    }

    pub fn clear_modal_chain(&mut self) {
        self.modal = None;
        self.modal_parents.clear();
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

    #[must_use]
    pub fn save_refs(&self) -> SettingsTrustSaveRefs<'_> {
        SettingsTrustSaveRefs {
            pending: &self.pending,
        }
    }

    pub fn discard(&mut self) {
        self.pending = self.original.clone();
        self.selected = self.selected.min(self.pending.len().saturating_sub(1));
        self.error = None;
    }

    pub fn apply_selection_plan(
        &mut self,
        plan: crate::tui::screens::settings::update::SettingsSelectionScrollPlan,
    ) {
        self.selected = plan.selected;
        self.scroll_y = plan.scroll_y;
    }

    pub fn apply_row_select_plan(
        &mut self,
        plan: crate::tui::screens::settings::update::SettingsTrustRowSelectPlan,
    ) -> bool {
        if let Some(selected) = plan.selected {
            self.selected = selected;
        }
        plan.content_focused
    }

    pub fn apply_horizontal_scroll(&mut self, scroll_x: u16) {
        self.scroll_x = scroll_x;
    }

    pub fn toggle_selected(&mut self) {
        if let Some(row) = self.pending.get_mut(self.selected) {
            row.trusted = !row.trusted;
        }
    }

    pub fn mark_saved(&mut self) {
        self.original = self.pending.clone();
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }
}

impl SettingsPanelTakeError for SettingsTrustState {
    fn take_panel_error(&mut self) -> Option<String> {
        self.take_error()
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

#[derive(Debug, Default, Clone, PartialEq, Eq)]
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

impl<
    TextInputState,
    SourcePickerState,
    OpPickerState,
    FileBrowserState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
> crate::tui::model::ConsoleAnimationTick
    for SettingsAuthModal<
        TextInputState,
        SourcePickerState,
        OpPickerState,
        FileBrowserState,
        AuthFormTarget,
        AuthForm,
        AuthFormFocus,
    >
where
    OpPickerState: crate::tui::model::ConsoleAnimationTick,
{
    fn tick_active_animation(&mut self) -> bool {
        match self {
            Self::OpPicker { state } => state.tick_active_animation(),
            Self::TextInput { .. }
            | Self::SourcePicker { .. }
            | Self::SourceFolderPicker { .. }
            | Self::AuthForm { .. } => false,
        }
    }
}

impl<
    TextInputState,
    SourcePickerState,
    OpPickerState,
    FileBrowserState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
>
    SettingsAuthModal<
        TextInputState,
        SourcePickerState,
        OpPickerState,
        FileBrowserState,
        AuthFormTarget,
        AuthForm,
        AuthFormFocus,
    >
where
    OpPickerState: ModalOpPickerState,
    AuthForm: ModalAuthFormState,
{
    #[must_use]
    pub fn rect_mode(&self) -> ModalRectMode {
        match self {
            Self::TextInput { .. } => ModalRectMode::TextInput,
            Self::SourcePicker { .. } => ModalRectMode::SourcePicker,
            Self::OpPicker { state } if state.has_naming_stage_input() => ModalRectMode::TextInput,
            Self::OpPicker { .. } => ModalRectMode::OpPicker,
            Self::SourceFolderPicker { .. } => ModalRectMode::FileBrowser,
            Self::AuthForm { state, .. } => ModalRectMode::AuthForm {
                required_height: state.required_height(),
            },
        }
    }

    #[must_use]
    pub const fn scroll_target(&self) -> crate::tui::update::SettingsAuthModalScrollTarget {
        use crate::tui::update::SettingsAuthModalScrollTarget;
        match self {
            Self::OpPicker { .. } => SettingsAuthModalScrollTarget::OpPicker,
            _ => SettingsAuthModalScrollTarget::None,
        }
    }

    #[must_use]
    pub fn footer_items(&self, can_generate_token: bool) -> Vec<jackin_tui::HintSpan<'static>>
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
            Self::TextInput { .. } => footer_items_for_mode(ModalFooterMode::ConfirmDismiss),
            Self::SourcePicker { .. } => footer_items_for_mode(ModalFooterMode::SegmentedChoice),
            Self::SourceFolderPicker { state } => state.footer_items(),
            Self::OpPicker { state } => footer_items_for_mode(state.footer_mode(false)),
        }
    }
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

    #[must_use]
    pub fn selected_detail_row_is_focusable(&self) -> bool {
        let Some(kind) = self.selected_kind else {
            return true;
        };
        let Some(row) = self.pending.iter().find(|row| row.kind == kind) else {
            return false;
        };
        crate::tui::screens::settings::update::settings_auth_detail_rows(kind, row.mode)
            .get(self.selected)
            .copied()
            .is_some_and(crate::tui::screens::settings::update::settings_auth_row_is_focusable)
    }

    #[must_use]
    pub const fn selected_kind(&self) -> Option<AuthKind> {
        self.selected_kind
    }

    #[must_use]
    pub const fn has_selected_kind(&self) -> bool {
        self.selected_kind.is_some()
    }

    pub const fn scroll_y_mut(&mut self) -> &mut u16 {
        &mut self.scroll_y
    }

    #[must_use]
    pub fn save_refs(&self) -> SettingsAuthSaveRefs<'_, EnvValue> {
        SettingsAuthSaveRefs {
            pending: &self.pending,
            original_github_env: &self.original_github_env,
            github_env: &self.github_env,
        }
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
        self.modal = self.pop_parent_modal();
    }

    #[must_use]
    pub const fn has_modal(&self) -> bool {
        self.modal.is_some()
    }

    #[must_use]
    pub const fn modal_ref(&self) -> Option<&Modal> {
        self.modal.as_ref()
    }

    pub const fn modal_mut(&mut self) -> Option<&mut Modal> {
        self.modal.as_mut()
    }

    pub fn take_modal(&mut self) -> Option<Modal> {
        self.modal.take()
    }

    pub fn set_modal(&mut self, modal: Modal) {
        self.modal = Some(modal);
    }

    pub fn clear_modal(&mut self) {
        self.modal = None;
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    pub const fn start_generating_token(&mut self) {
        self.generating_token = true;
    }

    pub const fn finish_generating_token(&mut self) {
        self.generating_token = false;
    }

    #[must_use]
    pub const fn is_generating_token(&self) -> bool {
        self.generating_token
    }

    pub fn set_pending_op_commit(&mut self, pending: PendingOpCommit) {
        self.pending_op_commit = Some(pending);
    }

    pub const fn pending_op_commit_mut(&mut self) -> Option<&mut PendingOpCommit> {
        self.pending_op_commit.as_mut()
    }

    pub fn take_pending_op_commit(&mut self) -> Option<PendingOpCommit> {
        self.pending_op_commit.take()
    }

    pub fn clamp_selected_row(&mut self) {
        self.selected = crate::tui::screens::settings::update::settings_auth_selected_index(
            self.selected,
            self.row_count(),
        );
    }

    pub const fn clear_selected_kind(&mut self) {
        self.selected_kind = None;
        self.selected = 0;
    }

    pub fn enter_selected_kind(&mut self) {
        if let Some(row) = self.pending.get(self.selected) {
            self.selected_kind = Some(row.kind);
            self.selected = 0;
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        let rows = self
            .selected_kind
            .and_then(|kind| {
                self.pending.iter().find(|row| row.kind == kind).map(|row| {
                    crate::tui::screens::settings::update::settings_auth_detail_rows(kind, row.mode)
                })
            })
            .unwrap_or_else(|| {
                (0..self.pending.len())
                    .map(|_| crate::tui::screens::settings::update::SettingsAuthDetailRow::Mode)
                    .collect()
            });
        self.selected = crate::tui::screens::settings::update::settings_auth_selection_plan(
            self.selected,
            &rows,
            delta,
        );
    }

    pub fn open_child_modal(&mut self, parent_modal: Modal, child_modal: Modal) {
        self.modal_parents.push(parent_modal);
        self.modal = Some(child_modal);
    }

    pub fn pop_parent_modal(&mut self) -> Option<Modal> {
        self.modal_parents.pop()
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

impl<EnvValue, Modal, PendingOpCommit> SettingsPanelTakeError
    for SettingsAuthState<EnvValue, Modal, PendingOpCommit>
{
    fn take_panel_error(&mut self) -> Option<String> {
        self.take_error()
    }
}

impl<EnvValue, Modal, PendingOpCommit> SettingsAuthRestorePendingForm
    for SettingsAuthState<EnvValue, Modal, PendingOpCommit>
{
    fn restore_pending_auth_form(&mut self) {
        Self::restore_pending_auth_form(self);
    }
}

impl<EnvValue, Modal, OpRef> crate::tui::model::ConsolePendingOpCommit
    for SettingsAuthState<EnvValue, Modal, crate::tui::subscriptions::PendingOpCommit<OpRef>>
{
    type OpRef = OpRef;

    fn poll_pending_op_commit(&mut self) -> Option<(Self::OpRef, anyhow::Result<()>)> {
        use jackin_tui::runtime::{Subscription, SubscriptionPoll};

        let pending = self.pending_op_commit.as_mut()?;
        let result = match pending.rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => Err(anyhow::anyhow!(
                crate::tui::subscriptions::op_read_worker_disconnected_message()
            )),
        };
        let pending = self.pending_op_commit.take()?;
        Some((pending.op_ref, result))
    }
}

impl<EnvValue, Modal, PendingOpCommit> SettingsAuthModalSlot
    for SettingsAuthState<EnvValue, Modal, PendingOpCommit>
{
    type Modal = Modal;

    fn modal_mut(&mut self) -> Option<&mut Self::Modal> {
        self.modal.as_mut()
    }
}

impl<Modal, PendingOpCommit> SettingsAuthState<jackin_core::EnvValue, Modal, PendingOpCommit> {
    pub fn open_selected_auth_modal(
        &mut self,
        agent_env: &BTreeMap<String, jackin_core::EnvValue>,
        build: impl FnOnce(
            AuthKind,
            &SettingsAuthRow<AuthKind, AuthMode>,
            Option<jackin_core::EnvValue>,
        ) -> Modal,
    ) {
        let Some(kind) = self.selected_kind else {
            return;
        };
        let Some(row) = self.pending.iter().find(|row| row.kind == kind) else {
            return;
        };
        let existing_credential = crate::tui::auth_config::settings_auth_env_value(
            kind,
            row.mode,
            &self.github_env,
            agent_env,
        )
        .cloned();
        self.modal = Some(build(kind, row, existing_credential));
    }

    pub fn apply_auth_outcome(
        &mut self,
        kind: AuthKind,
        outcome: crate::tui::components::auth_panel::AuthFormOutcome<jackin_core::EnvValue>,
        agent_env: &mut BTreeMap<String, jackin_core::EnvValue>,
    ) {
        if let Some(row) = self.pending.iter_mut().find(|row| row.kind == kind) {
            row.mode = outcome.mode;
            row.sync_source_dir = outcome.source_folder;
        }
        crate::tui::auth_config::apply_settings_auth_env_commit(
            kind,
            outcome.env_var_name,
            outcome.env_value,
            &mut self.github_env,
            agent_env,
        );
        self.clamp_selected_row();
    }

    pub fn clear_auth_kind(
        &mut self,
        kind: AuthKind,
        agent_env: &mut BTreeMap<String, jackin_core::EnvValue>,
    ) {
        if let Some(row) = self.pending.iter_mut().find(|row| row.kind == kind) {
            row.mode = AuthMode::Sync;
            row.sync_source_dir = None;
        }
        crate::tui::auth_config::clear_settings_auth_env_values(
            kind,
            &mut self.github_env,
            agent_env,
        );
    }
}

impl<
    MountRow,
    MountModal,
    EnvValue,
    EnvModal,
    AuthValue,
    AuthModal,
    PendingOpCommit,
    Trust,
    ErrorPopup,
    PendingToken,
> crate::tui::model::ConsoleSettingsModalPresence
    for SettingsState<
        GlobalMountsState<MountRow, MountModal>,
        SettingsEnvState<EnvValue, EnvModal>,
        SettingsAuthState<AuthValue, AuthModal, PendingOpCommit>,
        Trust,
        ErrorPopup,
        PendingToken,
    >
{
    fn settings_modal_facts(&self) -> crate::tui::model::ConsoleStageModalFacts {
        crate::tui::model::ConsoleStageModalFacts {
            settings_error_popup_open: self.error_popup.is_some(),
            settings_mounts_modal_open: self.mounts.modal.is_some(),
            settings_env_modal_open: self.env.modal.is_some(),
            settings_auth_modal_open: self.auth.has_modal(),
            ..crate::tui::model::ConsoleStageModalFacts::default()
        }
    }
}

impl<
    MountRow,
    MountModal,
    EnvValue,
    EnvModal,
    AuthValue,
    AuthModal,
    PendingOpCommit,
    Trust,
    ErrorPopup,
    PendingToken,
> crate::tui::model::ConsoleSettingsFooterHeight
    for SettingsState<
        GlobalMountsState<MountRow, MountModal>,
        SettingsEnvState<EnvValue, EnvModal>,
        SettingsAuthState<AuthValue, AuthModal, PendingOpCommit>,
        Trust,
        ErrorPopup,
        PendingToken,
    >
{
    fn settings_cached_footer_height(&self) -> u16 {
        self.cached_footer_h
    }
}

impl<
    MountRow,
    MountModal,
    EnvValue,
    EnvModal,
    AuthValue,
    AuthModal,
    PendingOpCommit,
    Trust,
    ErrorPopup,
    PendingToken,
> crate::tui::debug::ConsoleSettingsDebugFacts
    for SettingsState<
        GlobalMountsState<MountRow, MountModal>,
        SettingsEnvState<EnvValue, EnvModal>,
        SettingsAuthState<AuthValue, AuthModal, PendingOpCommit>,
        Trust,
        ErrorPopup,
        PendingToken,
    >
where
    MountModal: crate::tui::debug::ConsoleSettingsMountModalDebugKind,
{
    fn settings_stage_debug(&self) -> crate::tui::debug::ConsoleStageDebug {
        crate::tui::debug::ConsoleStageDebug::Settings {
            tab: format!("{:?}", self.active_tab),
            selected: self.mounts.selected,
            modal: self
                .mounts
                .modal
                .as_ref()
                .map(crate::tui::debug::ConsoleSettingsMountModalDebugKind::settings_mount_modal_debug_kind),
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsGeneralSaveRefs {
    pub git_coauthor_trailer: bool,
    pub git_dco: bool,
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

    #[must_use]
    pub const fn save_refs(&self) -> SettingsGeneralSaveRefs {
        SettingsGeneralSaveRefs {
            git_coauthor_trailer: self.pending_coauthor_trailer,
            git_dco: self.pending_dco,
        }
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

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = crate::tui::focus::moved_selection(self.selected, 2, delta);
    }

    pub const fn toggle_selected(&mut self) {
        match self.selected {
            0 => {
                self.pending_coauthor_trailer = !self.pending_coauthor_trailer;
            }
            1 => {
                self.pending_dco = !self.pending_dco;
            }
            _ => {}
        }
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
    use jackin_tui::components::{ErrorPopupState, FocusOwner};

    use crate::tui::components::footer_hints::{
        ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalFileBrowserFooterState,
        ModalFooterMode, ModalOpPickerFooterState,
    };
    use crate::tui::components::modal_rects::{
        ModalAuthFormState, ModalConfirmSaveState, ModalConfirmState, ModalOpPickerState,
        ModalRectMode, ModalRolePickerState,
    };

    use super::{
        GlobalMountsState, SettingsAfterEventOutcome, SettingsAuthRow, SettingsAuthState,
        SettingsEnvConfig, SettingsEnvRow, SettingsEnvScope, SettingsEnvState,
        SettingsGeneralSaveRefs, SettingsGeneralState, SettingsState, SettingsTrustRow,
        SettingsTrustState, settings_env_config_from_app_config,
        settings_trust_rows_from_app_config,
    };

    struct TestRolePicker(usize);

    impl ModalRolePickerState for TestRolePicker {
        fn filtered_len(&self) -> usize {
            self.0
        }
    }

    struct TestConfirm;

    fn minimal_settings_state()
    -> SettingsState<(), (), SettingsAuthState<String, &'static str, ()>, (), ErrorPopupState, ()>
    {
        SettingsState {
            active_tab: super::SettingsTab::General,
            focus_owner: FocusOwner::TabBar,
            hover_target: None,
            general: SettingsGeneralState::from_values(false, false),
            mounts: (),
            env: (),
            auth: SettingsAuthState::from_rows_and_github_env(Vec::new(), BTreeMap::new()),
            trust: (),
            error_popup: None,
            pending_token_generate: None,
            cached_footer_h: 1,
        }
    }

    #[test]
    fn settings_error_popup_open_and_dismiss_live_on_state() {
        let mut state = minimal_settings_state();

        state.open_error_popup("Settings error", "bad value");
        assert!(state.error_popup.is_some());

        state.auth.modal = Some("child");
        state.auth.modal_parents.push("parent");
        state.dismiss_error_popup();

        assert!(state.error_popup.is_none());
        assert_eq!(state.auth.modal, Some("parent"));
    }

    impl ModalConfirmState for TestConfirm {
        fn width_pct(&self) -> u16 {
            42
        }

        fn required_height(&self) -> u16 {
            9
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

    struct TestOpPicker(bool);

    impl ModalOpPickerState for TestOpPicker {
        fn has_naming_stage_input(&self) -> bool {
            self.0
        }
    }

    impl ModalOpPickerFooterState for TestOpPicker {
        fn footer_mode(&self, include_refresh: bool) -> ModalFooterMode {
            ModalFooterMode::FilteredPicker {
                include_refresh,
                include_collapse: false,
            }
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
                focus: super::AuthFormFocus::Mode,
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

    fn empty_env_config<V>() -> SettingsEnvConfig<V> {
        SettingsEnvConfig {
            env: BTreeMap::new(),
            roles: BTreeMap::new(),
        }
    }

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
    fn settings_env_pop_modal_chain_clears_pending_key_only_when_closed() {
        let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());
        state.modal = Some(2);
        state.modal_parents.push(1);
        state.pending_env_key = Some((SettingsEnvScope::Global, "KEY".into()));
        state.pending_picker_value = Some("value".into());

        state.pop_modal_chain_and_clear_pending_env_key_if_closed();

        assert_eq!(state.modal, Some(1));
        assert!(state.pending_env_key.is_some());
        assert!(state.pending_picker_value.is_some());

        state.pop_modal_chain_and_clear_pending_env_key_if_closed();

        assert!(state.modal.is_none());
        assert!(state.pending_env_key.is_none());
        assert!(state.pending_picker_value.is_none());
    }

    #[test]
    fn settings_env_pop_modal_chain_can_clear_pending_key_immediately() {
        let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());
        state.modal = Some(2);
        state.modal_parents.push(1);
        state.pending_env_key = Some((SettingsEnvScope::Global, "KEY".into()));
        state.pending_picker_value = Some("value".into());

        state.pop_modal_chain_and_clear_pending_env_key();

        assert_eq!(state.modal, Some(1));
        assert!(state.pending_env_key.is_none());
        assert!(state.pending_picker_value.is_none());
    }

    #[test]
    fn settings_env_pop_modal_chain_can_clear_picker_target_immediately() {
        let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());
        state.modal = Some(2);
        state.modal_parents.push(1);
        state.pending_picker_target = Some((SettingsEnvScope::Global, Some("KEY".into())));
        state.pending_picker_value = Some("value".into());

        state.pop_modal_chain_and_clear_picker_target();

        assert_eq!(state.modal, Some(1));
        assert!(state.pending_picker_target.is_none());
        assert!(state.pending_picker_value.is_none());
    }

    #[test]
    fn settings_env_set_pending_picker_target_stores_scope_and_key() {
        let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());

        state.set_pending_picker_target((SettingsEnvScope::Global, Some("KEY".into())));

        assert_eq!(
            state.pending_picker_target,
            Some((SettingsEnvScope::Global, Some(String::from("KEY"))))
        );
    }

    #[test]
    fn settings_env_set_pending_env_key_stores_scope_and_key() {
        let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());

        state.set_pending_env_key(SettingsEnvScope::Global, "KEY".into());

        assert_eq!(
            state.pending_env_key,
            Some((SettingsEnvScope::Global, String::from("KEY")))
        );
    }

    #[test]
    fn settings_env_clear_pending_env_key_removes_scope_and_key() {
        let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());
        state.set_pending_env_key(SettingsEnvScope::Global, "KEY".into());

        state.clear_pending_env_key();

        assert!(state.pending_env_key.is_none());
    }

    #[test]
    fn settings_env_clear_pending_picker_target_removes_scope_and_key() {
        let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());
        state.set_pending_picker_target((SettingsEnvScope::Global, Some("KEY".into())));

        state.clear_pending_picker_target();

        assert!(state.pending_picker_target.is_none());
    }

    #[test]
    fn settings_env_stash_pending_picker_value_stores_value() {
        let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());

        state.stash_pending_picker_value("value".into());

        assert_eq!(state.pending_picker_value, Some(String::from("value")));
    }

    #[test]
    fn settings_env_take_pending_picker_value_moves_value() {
        let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());
        state.stash_pending_picker_value("value".into());

        assert!(state.has_pending_picker_value());
        assert_eq!(
            state.take_pending_picker_value(),
            Some(String::from("value"))
        );
        assert!(!state.has_pending_picker_value());
    }

    #[test]
    fn settings_env_set_value_updates_global_env() {
        let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());

        state.set_value(&SettingsEnvScope::Global, "KEY", "value".into());

        assert_eq!(state.pending.env.get("KEY"), Some(&String::from("value")));
    }

    #[test]
    fn settings_env_expand_role_tracks_role() {
        let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());

        state.expand_role("default".into());

        assert!(state.expanded.contains("default"));
    }

    #[test]
    fn settings_env_set_and_take_error_moves_error_message() {
        let mut state = SettingsEnvState::<String, i32>::from_pending(empty_env_config());

        state.set_error("missing role");

        assert_eq!(state.take_error(), Some(String::from("missing role")));
        assert!(state.take_error().is_none());
    }

    #[test]
    fn settings_env_remove_selected_row_deletes_key_and_clamps_selection() {
        let mut config = SettingsEnvConfig {
            env: BTreeMap::from([
                ("A".into(), String::from("1")),
                ("B".into(), String::from("2")),
            ]),
            roles: BTreeMap::new(),
        };
        let mut state = SettingsEnvState::<String, i32>::from_pending(config.clone());
        let rows =
            crate::tui::screens::settings::update::settings_env_flat_rows(&config, &state.expanded);
        state.selected = rows
            .iter()
            .position(|row| {
                matches!(
                    row,
                    SettingsEnvRow::Key {
                        scope: SettingsEnvScope::Global,
                        key,
                    } if key == "B"
                )
            })
            .expect("B row should exist");

        assert!(state.remove_selected_row());

        config.env.remove("B");
        assert_eq!(state.pending, config);
        assert!(state.selected < state.pending.env.len() + 1);
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
    fn settings_trust_set_and_take_error_moves_error_message() {
        let mut state = SettingsTrustState::from_rows(Vec::new());

        state.set_error("trust failed");

        assert_eq!(state.take_error(), Some(String::from("trust failed")));
        assert!(state.take_error().is_none());
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
    fn global_mounts_start_add_draft_resets_draft_and_parent_chain() {
        let mut state = GlobalMountsState::<String, i32>::from_rows(Vec::new());
        state.modal = Some(2);
        state.modal_parents.push(1);
        state.add_draft = None;

        state.start_add_draft();

        assert!(state.add_draft.is_some());
        assert!(state.modal_parents.is_empty());
        assert_eq!(state.modal, Some(2));
    }

    #[test]
    fn global_mounts_remove_row_and_select_updates_pending_and_selection() {
        let mut state =
            GlobalMountsState::<String, i32>::from_rows(vec!["one".into(), "two".into()]);

        state.remove_row_and_select(0, 0);

        assert_eq!(state.pending, vec![String::from("two")]);
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn global_mounts_set_and_take_error_moves_error_message() {
        let mut state = GlobalMountsState::<String, i32>::from_rows(Vec::new());

        state.set_error("missing mount");

        assert_eq!(state.take_error(), Some(String::from("missing mount")));
        assert!(state.take_error().is_none());
    }

    #[test]
    fn global_mounts_request_and_take_exit_flag() {
        let mut state = GlobalMountsState::<String, i32>::from_rows(Vec::new());

        state.request_exit();

        assert!(state.take_exit_requested());
        assert!(!state.take_exit_requested());
    }

    #[test]
    fn global_mounts_add_row_and_close_updates_pending_selection_and_modal() {
        let mut state = GlobalMountsState::<GlobalMountRow, i32>::from_rows(Vec::new());
        state.modal = Some(1);
        state.modal_parents.push(0);
        let row = GlobalMountRow {
            scope: None,
            name: "cache".into(),
            mount: MountConfig {
                src: "/tmp/cache".into(),
                dst: "/home/agent/.cache".into(),
                readonly: false,
                isolation: jackin_core::isolation::MountIsolation::Shared,
            },
        };

        state.add_row_and_close(row, 0);

        assert_eq!(state.pending.len(), 1);
        assert_eq!(state.selected, 0);
        assert!(state.modal.is_none());
        assert!(state.modal_parents.is_empty());
    }

    #[test]
    fn global_mounts_toggle_selected_readonly_updates_selected_row() {
        let mut state = GlobalMountsState::<GlobalMountRow, i32>::from_rows(vec![
            GlobalMountRow {
                scope: None,
                name: "cache".into(),
                mount: MountConfig {
                    src: "/tmp/cache".into(),
                    dst: "/home/agent/.cache".into(),
                    readonly: false,
                    isolation: jackin_core::isolation::MountIsolation::Shared,
                },
            },
            GlobalMountRow {
                scope: None,
                name: "logs".into(),
                mount: MountConfig {
                    src: "/tmp/logs".into(),
                    dst: "/home/agent/logs".into(),
                    readonly: true,
                    isolation: jackin_core::isolation::MountIsolation::Shared,
                },
            },
        ]);
        state.selected = 1;

        state.toggle_selected_readonly();

        assert!(!state.pending[1].mount.readonly);
        assert!(!state.pending[0].mount.readonly);
    }

    #[test]
    fn global_mounts_pop_modal_chain_preserves_add_draft_when_parent_remains() {
        let mut state = GlobalMountsState::<String, i32>::from_rows(Vec::new());
        state.modal = Some(2);
        state.modal_parents.push(1);
        state.add_draft = Some(super::GlobalMountDraft::default());

        state.pop_modal_chain_and_clear_add_draft_if_closed();

        assert_eq!(state.modal, Some(1));
        assert!(state.add_draft.is_some());
    }

    #[test]
    fn global_mounts_pop_modal_chain_clears_add_draft_when_closed() {
        let mut state = GlobalMountsState::<String, i32>::from_rows(Vec::new());
        state.modal = Some(1);
        state.add_draft = Some(super::GlobalMountDraft::default());

        state.pop_modal_chain_and_clear_add_draft_if_closed();

        assert_eq!(state.modal, None);
        assert!(state.add_draft.is_none());
    }

    #[test]
    fn global_mount_modal_reports_debug_kind() {
        type TestModal = super::GlobalMountModal<(), (), (), (), (), (), ()>;

        let modal = TestModal::Confirm {
            action: super::GlobalMountConfirm::Sensitive,
            state: (),
        };

        assert_eq!(
            modal.debug_kind(),
            crate::tui::debug::SettingsMountModalDebugKind::ConfirmSensitive
        );
    }

    #[test]
    fn settings_env_modal_reports_scroll_target() {
        type TestModal =
            super::SettingsEnvModal<(), (), TestOpPicker, TestRolePicker, (), TestConfirm>;

        assert_eq!(
            TestModal::OpPicker {
                state: Box::new(TestOpPicker(false)),
            }
            .scroll_target(),
            crate::tui::update::SettingsEnvModalScrollTarget::OpPicker
        );
        assert_eq!(
            TestModal::RolePicker {
                state: TestRolePicker(7),
            }
            .scroll_target(),
            crate::tui::update::SettingsEnvModalScrollTarget::RolePicker
        );
        assert_eq!(
            TestModal::SourcePicker { state: () }.scroll_target(),
            crate::tui::update::SettingsEnvModalScrollTarget::None
        );
    }

    #[test]
    fn global_mount_modal_reports_scroll_target() {
        type TestModal =
            super::GlobalMountModal<(), (), (), (), TestRolePicker, TestConfirm, TestConfirmSave>;

        assert_eq!(
            TestModal::RolePicker {
                state: TestRolePicker(7),
            }
            .scroll_target(),
            crate::tui::update::GlobalMountModalScrollTarget::RolePicker
        );
        assert_eq!(
            TestModal::ScopePicker { state: () }.scroll_target(),
            crate::tui::update::GlobalMountModalScrollTarget::None
        );
    }

    #[test]
    fn global_mount_modal_reports_letter_input_kind() {
        type TestModal =
            super::GlobalMountModal<(), (), (), (), TestRolePicker, TestConfirm, TestConfirmSave>;

        assert_eq!(
            TestModal::Text {
                target: super::GlobalMountTextTarget::AddName,
                state: Box::new(()),
            }
            .letter_input_kind(),
            Some(crate::tui::run::LetterInputModalKind::TextInput)
        );
        assert_eq!(
            TestModal::RolePicker {
                state: TestRolePicker(7),
            }
            .letter_input_kind(),
            Some(crate::tui::run::LetterInputModalKind::Other)
        );
    }

    #[test]
    fn settings_auth_modal_reports_scroll_target() {
        type TestModal = super::SettingsAuthModal<(), (), TestOpPicker, (), (), TestAuthForm, ()>;

        assert_eq!(
            TestModal::OpPicker {
                state: Box::new(TestOpPicker(false)),
            }
            .scroll_target(),
            crate::tui::update::SettingsAuthModalScrollTarget::OpPicker
        );
        assert_eq!(
            TestModal::SourcePicker { state: () }.scroll_target(),
            crate::tui::update::SettingsAuthModalScrollTarget::None
        );
    }

    #[test]
    fn settings_env_modal_reports_rect_mode() {
        type TestModal =
            super::SettingsEnvModal<(), (), TestOpPicker, TestRolePicker, (), TestConfirm>;

        let modal = TestModal::RolePicker {
            state: TestRolePicker(7),
        };

        assert_eq!(
            modal.rect_mode(),
            ModalRectMode::RolePicker { filtered_len: 7 }
        );
    }

    #[test]
    fn settings_env_op_naming_modal_uses_text_input_rect_mode() {
        type TestModal =
            super::SettingsEnvModal<(), (), TestOpPicker, TestRolePicker, (), TestConfirm>;

        let modal = TestModal::OpPicker {
            state: Box::new(TestOpPicker(true)),
        };

        assert_eq!(modal.rect_mode(), ModalRectMode::TextInput);
    }

    #[test]
    fn global_mount_modal_reports_rect_mode() {
        type TestModal =
            super::GlobalMountModal<(), (), (), (), TestRolePicker, TestConfirm, TestConfirmSave>;

        let modal = TestModal::PreviewSave {
            state: TestConfirmSave,
        };

        assert_eq!(
            modal.rect_mode(),
            ModalRectMode::ConfirmSave {
                required_height: 12
            }
        );
    }

    #[test]
    fn settings_auth_modal_reports_rect_mode() {
        type TestModal = super::SettingsAuthModal<(), (), TestOpPicker, (), (), TestAuthForm, ()>;

        let modal = TestModal::AuthForm {
            target: (),
            state: Box::new(TestAuthForm),
            focus: (),
            literal_buffer: String::new(),
        };

        assert_eq!(
            modal.rect_mode(),
            ModalRectMode::AuthForm {
                required_height: 13
            }
        );
    }

    #[test]
    fn settings_modals_report_footer_items() {
        type EnvModal =
            super::SettingsEnvModal<(), (), TestOpPicker, TestRolePicker, (), TestConfirm>;
        type MountModal = super::GlobalMountModal<
            (),
            TestFileBrowser,
            (),
            (),
            TestRolePicker,
            TestConfirm,
            TestConfirmSave,
        >;
        type AuthModal =
            super::SettingsAuthModal<(), (), TestOpPicker, TestFileBrowser, (), TestAuthForm, ()>;

        assert!(
            EnvModal::RolePicker {
                state: TestRolePicker(3),
            }
            .footer_items()
            .contains(&jackin_tui::HintSpan::Text("filter"))
        );

        assert!(
            MountModal::PreviewSave {
                state: TestConfirmSave,
            }
            .footer_items()
            .contains(&jackin_tui::HintSpan::Text("save"))
        );

        assert!(
            AuthModal::AuthForm {
                target: (),
                state: Box::new(TestAuthForm),
                focus: (),
                literal_buffer: String::new(),
            }
            .footer_items(true)
            .contains(&jackin_tui::HintSpan::Text("generate"))
        );
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
    fn settings_auth_state_reports_selected_detail_focusability() {
        let rows = vec![SettingsAuthRow {
            kind: crate::tui::auth::AuthKind::Github,
            mode: crate::tui::auth::AuthMode::Token,
            sync_source_dir: None,
        }];
        let mut state =
            SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());

        assert!(state.selected_detail_row_is_focusable());

        state.selected_kind = Some(crate::tui::auth::AuthKind::Github);
        state.selected = 1;

        assert!(!state.selected_detail_row_is_focusable());
    }

    #[test]
    fn settings_auth_set_and_take_error_moves_error_message() {
        let mut state = SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(
            Vec::new(),
            BTreeMap::new(),
        );

        state.set_error("auth failed");

        assert_eq!(state.take_error(), Some(String::from("auth failed")));
        assert!(state.take_error().is_none());
    }

    #[test]
    fn settings_auth_token_generation_flag_can_start_and_finish() {
        let mut state = SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(
            Vec::new(),
            BTreeMap::new(),
        );

        assert!(!state.is_generating_token());

        state.start_generating_token();
        assert!(state.is_generating_token());

        state.finish_generating_token();
        assert!(!state.is_generating_token());
    }

    #[test]
    fn settings_auth_clamp_selected_row_uses_current_row_count() {
        let rows = vec![SettingsAuthRow {
            kind: crate::tui::auth::AuthKind::Github,
            mode: crate::tui::auth::AuthMode::Token,
            sync_source_dir: None,
        }];
        let mut state =
            SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());
        state.selected = 10;

        state.clamp_selected_row();

        assert_eq!(state.selected, 0);
    }

    #[test]
    fn settings_auth_child_modal_stack_round_trips_parent() {
        let mut state = SettingsAuthState::<EnvValue, i32, ()>::from_rows_and_github_env(
            Vec::new(),
            BTreeMap::new(),
        );

        state.open_child_modal(1, 2);

        assert_eq!(state.modal, Some(2));
        assert_eq!(state.pop_parent_modal(), Some(1));
        assert_eq!(state.pop_parent_modal(), None);
    }

    #[test]
    fn settings_auth_modal_slot_methods_round_trip() {
        let mut state = SettingsAuthState::<EnvValue, i32, ()>::from_rows_and_github_env(
            Vec::new(),
            BTreeMap::new(),
        );

        assert!(!state.has_modal());

        state.set_modal(7);

        assert!(state.has_modal());
        assert_eq!(state.modal_ref(), Some(&7));
        assert_eq!(state.modal_mut().map(|modal| *modal), Some(7));
        assert_eq!(state.take_modal(), Some(7));
        assert!(!state.has_modal());

        state.set_modal(9);
        state.clear_modal();

        assert!(!state.has_modal());
    }

    #[test]
    fn settings_auth_enter_and_clear_selected_kind_update_selection() {
        let rows = vec![SettingsAuthRow {
            kind: crate::tui::auth::AuthKind::Github,
            mode: crate::tui::auth::AuthMode::Sync,
            sync_source_dir: None,
        }];
        let mut state =
            SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());

        state.enter_selected_kind();

        assert_eq!(
            state.selected_kind,
            Some(crate::tui::auth::AuthKind::Github)
        );
        assert_eq!(state.selected, 0);

        state.clear_selected_kind();

        assert_eq!(state.selected_kind, None);
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn settings_auth_selected_kind_and_scroll_accessors_reflect_state() {
        let rows = vec![SettingsAuthRow {
            kind: crate::tui::auth::AuthKind::Github,
            mode: crate::tui::auth::AuthMode::Sync,
            sync_source_dir: None,
        }];
        let mut state =
            SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());

        assert_eq!(state.selected_kind(), None);
        assert!(!state.has_selected_kind());

        state.enter_selected_kind();
        *state.scroll_y_mut() = 3;

        assert_eq!(
            state.selected_kind(),
            Some(crate::tui::auth::AuthKind::Github)
        );
        assert!(state.has_selected_kind());
        assert_eq!(state.scroll_y, 3);
    }

    #[test]
    fn settings_auth_save_refs_expose_persisted_inputs() {
        let rows = vec![SettingsAuthRow {
            kind: crate::tui::auth::AuthKind::Github,
            mode: crate::tui::auth::AuthMode::Sync,
            sync_source_dir: None,
        }];
        let state = SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(
            rows,
            BTreeMap::from([(String::from("GH_TOKEN"), "token".into())]),
        );

        let refs = state.save_refs();

        assert_eq!(refs.pending.len(), 1);
        assert_eq!(
            refs.original_github_env.get("GH_TOKEN"),
            Some(&EnvValue::from("token"))
        );
        assert_eq!(
            refs.github_env.get("GH_TOKEN"),
            Some(&EnvValue::from("token"))
        );
    }

    #[test]
    fn settings_subpanel_save_refs_expose_persisted_inputs() {
        let mounts = GlobalMountsState::<i32, ()>::from_rows(vec![1, 2]);
        let env = SettingsEnvState::<EnvValue, ()>::from_pending(SettingsEnvConfig {
            env: BTreeMap::from([(String::from("KEY"), "value".into())]),
            roles: BTreeMap::new(),
        });
        let trust = SettingsTrustState::from_rows(vec![SettingsTrustRow {
            role: String::from("role"),
            git: String::from("https://example.test/repo.git"),
            trusted: true,
        }]);
        let general = SettingsGeneralState::from_values(true, false);

        let mount_refs = mounts.save_refs();
        let env_refs = env.save_refs();
        let trust_refs = trust.save_refs();
        let general_refs = general.save_refs();

        assert_eq!(mount_refs.original, &[1, 2]);
        assert_eq!(mount_refs.pending, &[1, 2]);
        assert_eq!(
            env_refs.original.env.get("KEY"),
            Some(&EnvValue::from("value"))
        );
        assert_eq!(
            env_refs.pending.env.get("KEY"),
            Some(&EnvValue::from("value"))
        );
        assert_eq!(trust_refs.pending[0].role, "role");
        assert_eq!(
            general_refs,
            SettingsGeneralSaveRefs {
                git_coauthor_trailer: true,
                git_dco: false,
            }
        );
    }

    #[test]
    fn settings_state_applies_tab_move_plan() {
        type TestState = SettingsState<
            GlobalMountsState<GlobalMountRow, ()>,
            SettingsEnvState<EnvValue, ()>,
            SettingsAuthState<EnvValue, (), ()>,
            SettingsTrustState,
            (),
            (),
        >;
        let mut state = TestState::from_config(&AppConfig::default());

        state.apply_tab_move_plan(crate::tui::screens::settings::update::SettingsTabMovePlan {
            active_tab: super::SettingsTab::Trust,
            tab_bar_focused: false,
        });

        assert_eq!(state.active_tab, super::SettingsTab::Trust);
        assert!(!state.tab_bar_focused());

        state.apply_tab_bar_focus_plan(true);
        assert!(state.tab_bar_focused());
    }

    #[test]
    fn settings_state_applies_scroll_focus_plan() {
        type TestState = SettingsState<
            GlobalMountsState<GlobalMountRow, ()>,
            SettingsEnvState<EnvValue, ()>,
            SettingsAuthState<EnvValue, (), ()>,
            SettingsTrustState,
            (),
            (),
        >;
        let mut state = TestState::from_config(&AppConfig::default());

        state.apply_scroll_focus_plan(
            crate::tui::screens::settings::update::SettingsScrollFocusPlan {
                mounts: false,
                env: true,
                auth: false,
                trust: false,
            },
        );

        assert!(state.content_focused(super::SettingsTab::Environments));
    }

    #[test]
    fn settings_state_applies_trust_row_select_plan_and_focus() {
        type TestState = SettingsState<
            GlobalMountsState<GlobalMountRow, ()>,
            SettingsEnvState<EnvValue, ()>,
            SettingsAuthState<EnvValue, (), ()>,
            SettingsTrustState,
            (),
            (),
        >;
        let mut state = TestState::from_config(&AppConfig::default());
        state.set_tab_bar_focused(true);

        state.apply_trust_row_select_plan(
            crate::tui::screens::settings::update::SettingsTrustRowSelectPlan {
                selected: Some(2),
                content_focused: true,
            },
        );

        assert_eq!(state.trust.selected, 2);
        assert!(state.content_focused(super::SettingsTab::Trust));

        state.set_hover_target(Some(super::SettingsHoverTarget::TrustRow(1)));
        assert_eq!(state.hovered_trust_row(), Some(1));
    }

    #[test]
    fn settings_general_state_moves_and_toggles_selection() {
        let mut state = SettingsGeneralState::from_values(false, false);

        state.move_selection(1);
        state.toggle_selected();

        assert_eq!(state.selected, 1);
        assert!(state.pending_dco);

        state.move_selection(-1);
        state.toggle_selected();

        assert_eq!(state.selected, 0);
        assert!(state.pending_coauthor_trailer);
    }

    #[test]
    fn settings_env_state_updates_role_expansion() {
        let mut state = SettingsEnvState::<EnvValue, ()>::from_pending(empty_env_config());

        state.set_role_expanded(String::from("alpha"), true);
        assert!(state.expanded.contains("alpha"));

        state.set_role_expanded(String::from("alpha"), false);
        assert!(!state.expanded.contains("alpha"));
    }

    #[test]
    fn settings_subpanels_apply_selection_plans() {
        let mut mounts = GlobalMountsState::<String, ()>::from_rows(Vec::new());
        let mut env = SettingsEnvState::<EnvValue, ()>::from_pending(empty_env_config());
        let mut trust = SettingsTrustState::from_rows(Vec::new());

        mounts.apply_selection_plan(
            crate::tui::screens::settings::update::SettingsSelectionScrollPlan {
                selected: 2,
                scroll_y: 3,
            },
        );
        env.apply_selection_plan(
            crate::tui::screens::settings::update::SettingsSelectionScrollPlan {
                selected: 4,
                scroll_y: 5,
            },
        );
        trust.apply_selection_plan(
            crate::tui::screens::settings::update::SettingsSelectionScrollPlan {
                selected: 6,
                scroll_y: 7,
            },
        );

        assert_eq!((mounts.selected, mounts.scroll_y), (2, 3));
        assert_eq!((env.selected, env.scroll_y), (4, 5));
        assert_eq!((trust.selected, trust.scroll_y), (6, 7));
    }

    #[test]
    fn settings_subpanels_apply_scroll_and_trust_row_plans() {
        let mut mounts = GlobalMountsState::<String, ()>::from_rows(Vec::new());
        let mut trust = SettingsTrustState::from_rows(Vec::new());

        mounts.apply_horizontal_scroll(8);
        trust.apply_horizontal_scroll(13);
        let content_focused = trust.apply_row_select_plan(
            crate::tui::screens::settings::update::SettingsTrustRowSelectPlan {
                selected: Some(3),
                content_focused: true,
            },
        );

        assert_eq!(mounts.scroll_x, 8);
        assert_eq!(trust.scroll_x, 13);
        assert_eq!(trust.selected, 3);
        assert!(content_focused);
    }

    #[test]
    fn settings_trust_state_toggles_selected_row() {
        let mut state = SettingsTrustState::from_rows(vec![SettingsTrustRow {
            role: String::from("role"),
            git: String::from("https://example.test/repo.git"),
            trusted: false,
        }]);

        state.toggle_selected();

        assert!(state.pending[0].trusted);
    }

    #[test]
    fn settings_auth_move_selection_uses_current_rows() {
        let rows = vec![
            SettingsAuthRow {
                kind: crate::tui::auth::AuthKind::Github,
                mode: crate::tui::auth::AuthMode::Sync,
                sync_source_dir: None,
            },
            SettingsAuthRow {
                kind: crate::tui::auth::AuthKind::Claude,
                mode: crate::tui::auth::AuthMode::Sync,
                sync_source_dir: None,
            },
        ];
        let mut state =
            SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());

        state.move_selection(1);

        assert_eq!(state.selected, 1);
    }

    #[test]
    fn settings_auth_pending_op_commit_round_trips() {
        let mut state = SettingsAuthState::<EnvValue, (), i32>::from_rows_and_github_env(
            Vec::new(),
            BTreeMap::new(),
        );

        state.set_pending_op_commit(7);

        assert_eq!(state.pending_op_commit_mut().copied(), Some(7));
        assert_eq!(state.take_pending_op_commit(), Some(7));
        assert_eq!(state.take_pending_op_commit(), None);
    }

    #[test]
    fn settings_auth_open_selected_modal_supplies_row_and_credential() {
        let rows = vec![SettingsAuthRow {
            kind: crate::tui::auth::AuthKind::Claude,
            mode: crate::tui::auth::AuthMode::ApiKey,
            sync_source_dir: None,
        }];
        let mut state =
            SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());
        state.selected_kind = Some(crate::tui::auth::AuthKind::Claude);
        let agent_env = BTreeMap::from([(
            String::from(jackin_core::env_model::ANTHROPIC_API_KEY_ENV_NAME),
            "secret".into(),
        )]);

        state.open_selected_auth_modal(&agent_env, |kind, row, existing| {
            assert_eq!(kind, crate::tui::auth::AuthKind::Claude);
            assert_eq!(row.mode, crate::tui::auth::AuthMode::ApiKey);
            assert_eq!(existing, Some(EnvValue::from("secret")));
        });

        assert_eq!(state.modal, Some(()));
    }

    #[test]
    fn settings_auth_apply_outcome_updates_row_and_env() {
        let rows = vec![SettingsAuthRow {
            kind: crate::tui::auth::AuthKind::Claude,
            mode: crate::tui::auth::AuthMode::Sync,
            sync_source_dir: None,
        }];
        let mut state =
            SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());
        let mut agent_env = BTreeMap::new();

        state.apply_auth_outcome(
            crate::tui::auth::AuthKind::Claude,
            crate::tui::components::auth_panel::AuthFormOutcome {
                mode: crate::tui::auth::AuthMode::ApiKey,
                env_var_name: Some(jackin_core::env_model::ANTHROPIC_API_KEY_ENV_NAME),
                env_value: Some("secret".into()),
                source_folder: None,
            },
            &mut agent_env,
        );

        assert_eq!(state.pending[0].mode, crate::tui::auth::AuthMode::ApiKey);
        assert_eq!(
            agent_env.get(jackin_core::env_model::ANTHROPIC_API_KEY_ENV_NAME),
            Some(&EnvValue::from("secret"))
        );
    }

    #[test]
    fn settings_auth_clear_kind_resets_row_and_env() {
        let rows = vec![SettingsAuthRow {
            kind: crate::tui::auth::AuthKind::Claude,
            mode: crate::tui::auth::AuthMode::ApiKey,
            sync_source_dir: Some(std::path::PathBuf::from("/tmp/auth")),
        }];
        let mut state =
            SettingsAuthState::<EnvValue, (), ()>::from_rows_and_github_env(rows, BTreeMap::new());
        let mut agent_env = BTreeMap::from([(
            String::from(jackin_core::env_model::ANTHROPIC_API_KEY_ENV_NAME),
            "secret".into(),
        )]);

        state.clear_auth_kind(crate::tui::auth::AuthKind::Claude, &mut agent_env);

        assert_eq!(state.pending[0].mode, crate::tui::auth::AuthMode::Sync);
        assert_eq!(state.pending[0].sync_source_dir, None);
        assert!(!agent_env.contains_key(jackin_core::env_model::ANTHROPIC_API_KEY_ENV_NAME));
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
    fn settings_state_after_event_outcome_drains_error_and_exit() {
        type TestState = SettingsState<
            GlobalMountsState<GlobalMountRow, ()>,
            SettingsEnvState<EnvValue, ()>,
            SettingsAuthState<EnvValue, (), ()>,
            SettingsTrustState,
            (),
            (),
        >;
        let mut state = TestState::from_config(&AppConfig::default());
        state.mounts.set_error("bad mount");
        state.env.set_error("bad env");
        state.auth.set_error("bad auth");
        state.trust.set_error("bad trust");
        state.mounts.request_exit();

        assert_eq!(
            state.take_after_event_outcome(),
            SettingsAfterEventOutcome {
                exit_requested: true,
                error: Some("bad mount".into()),
            }
        );
        assert_eq!(
            state.take_after_event_outcome(),
            SettingsAfterEventOutcome {
                exit_requested: false,
                error: Some("bad env".into()),
            }
        );
        assert_eq!(
            state.take_after_event_outcome(),
            SettingsAfterEventOutcome {
                exit_requested: false,
                error: Some("bad auth".into()),
            }
        );
        assert_eq!(
            state.take_after_event_outcome(),
            SettingsAfterEventOutcome {
                exit_requested: false,
                error: Some("bad trust".into()),
            }
        );
        assert_eq!(
            state.take_after_event_outcome(),
            SettingsAfterEventOutcome {
                exit_requested: false,
                error: None,
            }
        );
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
