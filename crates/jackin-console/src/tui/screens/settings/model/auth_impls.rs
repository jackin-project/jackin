/// `SettingsAuthState` impls + helper fns.
use super::*;

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
