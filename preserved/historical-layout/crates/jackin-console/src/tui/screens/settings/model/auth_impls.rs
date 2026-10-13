// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// `SettingsAuthState` impls + helper fns.
use super::{
    AccountScanOutcome, AccountScanState, AccountScanSummary, AuthKind, BTreeMap,
    GlobalMountsState, SettingsAuthRestorePendingForm, SettingsAuthSaveRefs, SettingsAuthSlot,
    SettingsAuthState, SettingsEnvState, SettingsPanelChangeCount, SettingsPanelDirty,
    SettingsPanelDiscard, SettingsPanelMarkSaved, SettingsPanelTakeError, SettingsState,
};
use crate::tui::screens::settings::effect::SettingsEffect;

impl<EnvValue, Modal, PendingOpCommit> SettingsAuthState<EnvValue, Modal, PendingOpCommit> {
    #[must_use]
    pub fn from_config(config: &jackin_config::AppConfig) -> Self {
        let mut state = Self::from_accounts(config.accounts.clone());
        state.github = config.github.clone().unwrap_or_default();
        state.original_github = state.github.clone();
        state.bindings = config.account_bindings.clone();
        state.original_bindings = state.bindings.clone();
        state
    }

    #[must_use]
    pub fn from_accounts(pending: BTreeMap<String, jackin_config::AccountConfig>) -> Self {
        Self {
            selected: 0,
            selected_kind: None,
            original: pending.clone(),
            pending,
            github: jackin_config::GithubAuthConfig::default(),
            original_github: jackin_config::GithubAuthConfig::default(),
            bindings: BTreeMap::new(),
            original_bindings: BTreeMap::new(),
            editing_account: None,
            editing_text: None,
            value_type: std::marker::PhantomData,
            modals: crate::tui::modal_chain::ModalChain::new(),
            error: None,
            pending_op_commit: None,
            scroll: crate::tui::scroll_block::console_scroll_area_state(),
            scan: AccountScanState::default(),
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.pending != self.original
            || self.github != self.original_github
            || self.bindings != self.original_bindings
    }

    #[must_use]
    pub fn row_count(&self) -> usize {
        // Pending accounts + "+ Add {kind}" rows + GitHub row + scan row.
        // The scan row stays last so the GitHub index
        // (`pending.len() + ACCOUNT_KINDS.len()`) never moves.
        self.pending.len() + ACCOUNT_KINDS.len() + 2
    }

    #[must_use]
    pub const fn selected_detail_row_is_focusable(&self) -> bool {
        true
    }

    #[must_use]
    pub const fn selected_kind(&self) -> Option<AuthKind> {
        self.selected_kind
    }

    #[must_use]
    pub const fn has_selected_kind(&self) -> bool {
        false
    }

    pub fn scroll_state_mut(&mut self) -> &mut termrock::widgets::ScrollAreaState {
        &mut self.scroll
    }

    #[must_use]
    pub fn save_refs(&self) -> SettingsAuthSaveRefs<'_> {
        SettingsAuthSaveRefs {
            pending: &self.pending,
            original: &self.original,
            github: &self.github,
            original_github: &self.original_github,
            bindings: &self.bindings,
            original_bindings: &self.original_bindings,
        }
    }

    pub fn discard(&mut self)
    where
        EnvValue: Clone,
    {
        self.pending = self.original.clone();
        self.github = self.original_github.clone();
        self.bindings = self.original_bindings.clone();
        self.editing_account = None;
        self.editing_text = None;
        self.selected_kind = None;
        self.selected = self.selected.min(self.pending.len().saturating_sub(1));
        self.modals.clear();
        self.error = None;
        // Discard orphans any in-flight scan: a stale completion must not
        // resurrect candidates the operator just threw away.
        let generation = self.scan.generation.wrapping_add(1);
        self.scan = AccountScanState {
            generation,
            ..AccountScanState::default()
        };
    }

    pub fn mark_saved(&mut self)
    where
        EnvValue: Clone,
    {
        self.original = self.pending.clone();
        self.original_github = self.github.clone();
        self.original_bindings = self.bindings.clone();
        self.scan.scanned_ids.clear();
        self.scan.issues.clear();
        self.scan.last_summary = None;
    }

    pub fn restore_pending_auth_form(&mut self) {
        self.modals.pop();
    }

    #[must_use]
    pub const fn has_modal(&self) -> bool {
        self.modals.is_open()
    }

    #[must_use]
    pub const fn modal_ref(&self) -> Option<&Modal> {
        self.modals.current()
    }

    pub fn modal_mut(&mut self) -> Option<&mut Modal> {
        self.modals.current_mut()
    }

    pub fn take_modal(&mut self) -> Option<Modal> {
        self.modals.take_current()
    }

    pub fn set_modal(&mut self, modal: Modal) {
        self.modals.set_current(modal);
    }

    pub fn clear_modal(&mut self) {
        self.modals.clear();
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
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
        let github_index = self.pending.len() + ACCOUNT_KINDS.len();
        self.selected_kind = self
            .pending
            .values()
            .nth(self.selected)
            .map(account_kind)
            .or_else(|| {
                ACCOUNT_KINDS
                    .get(self.selected.saturating_sub(self.pending.len()))
                    .copied()
                    .or_else(|| (self.selected == github_index).then_some(AuthKind::Github))
            });
    }

    pub fn move_selection(&mut self, delta: isize) {
        let count = self.row_count();
        if count > 0 {
            self.selected = self.selected.saturating_add_signed(delta).min(count - 1);
        }
    }

    pub fn toggle_selected_account_enabled(&mut self) {
        if let Some((id, account)) = self.pending.iter_mut().nth(self.selected) {
            account.enabled = !account.enabled;
            if !account.enabled {
                self.bindings.retain(|_, value| value != id);
            }
        }
    }

    pub fn toggle_account_default(
        &mut self,
        id: &str,
        agent: jackin_core::Agent,
    ) -> Result<(), String> {
        let account = self
            .pending
            .get(id)
            .ok_or_else(|| "Account no longer exists".to_owned())?;
        if !account.supports_agent(agent) {
            return Err("Choose an enabled account compatible with this agent".into());
        }
        if self.bindings.get(&agent).is_some_and(|value| value == id) {
            self.bindings.remove(&agent);
        } else {
            self.bindings.insert(agent, id.to_owned());
        }
        Ok(())
    }

    pub fn delete_selected_account(&mut self) {
        if let Some(id) = self.pending.keys().nth(self.selected).cloned() {
            self.pending.remove(&id);
            self.bindings.retain(|_, value| value != &id);
        }
        self.clamp_selected_row();
    }

    pub fn open_child_modal(&mut self, parent_modal: Modal, child_modal: Modal) {
        self.modals.open_pair(parent_modal, child_modal);
    }

    pub fn pop_parent_modal(&mut self) -> Option<Modal> {
        self.modals.pop();
        self.modals.take_current()
    }

    /// Push the current auth modal onto the parent stack so a sub-modal can
    /// open without losing the auth form's in-progress state.
    pub fn push_auth_modal(&mut self, sub_modal: Modal) {
        self.modals.open_sub(sub_modal);
    }

    /// Arm a scan worker run, returning the effect the root executes.
    /// `None` while a scan is already in flight (concurrent scans from
    /// the UI dedupe here; concurrent scans across processes dedupe
    /// under the config lock).
    pub fn begin_account_scan(&mut self) -> Option<SettingsEffect> {
        if self.scan.in_flight {
            return None;
        }
        self.scan.in_flight = true;
        self.scan.generation = self.scan.generation.wrapping_add(1);
        Some(SettingsEffect::StartAccountScan {
            generation: self.scan.generation,
        })
    }

    /// Abandon the in-flight scan; its late completion is ignored via
    /// the bumped generation (the worker thread itself cannot be
    /// recalled, only orphaned).
    pub fn cancel_account_scan(&mut self) {
        self.scan.in_flight = false;
        self.scan.generation = self.scan.generation.wrapping_add(1);
    }

    /// Merge a scan worker completion into the pending draft. Stale
    /// generations are ignored; worker failures surface as panel errors.
    pub fn complete_account_scan(
        &mut self,
        generation: u64,
        result: &Result<AccountScanOutcome, String>,
    ) {
        if generation != self.scan.generation {
            return;
        }
        self.scan.in_flight = false;
        match result {
            Err(error) => self.set_error(error.clone()),
            Ok(outcome) => {
                let summary = self.merge_account_scan_outcome(outcome);
                self.scan.issues = outcome.issues.clone();
                self.scan.last_summary = Some(summary);
            }
        }
    }

    /// Join a scan outcome into the draft: committed accounts (already on
    /// disk) refresh both pending and original, candidates join pending
    /// only so Apply commits them and Cancel preserves the pre-scan
    /// draft. IDs already present — including IDs the operator deleted
    /// from pending — are never touched, so newer edits are never
    /// silently overwritten.
    pub fn merge_account_scan_outcome(
        &mut self,
        outcome: &AccountScanOutcome,
    ) -> AccountScanSummary {
        let mut summary = AccountScanSummary {
            joined: Vec::new(),
            skipped: Vec::new(),
            fresh_install: outcome.fresh_install,
        };
        for (id, account) in &outcome.committed {
            if self.pending.contains_key(id) || self.original.contains_key(id) {
                summary.skipped.push(id.clone());
                continue;
            }
            self.original.insert(id.clone(), account.clone());
            self.pending.insert(id.clone(), account.clone());
            self.scan.scanned_ids.insert(id.clone());
            summary.joined.push(id.clone());
        }
        for (id, account) in &outcome.candidates {
            if self.pending.contains_key(id)
                || self.original.contains_key(id)
                || crate::tui::screens::settings::update::scanned_source_in_draft(
                    &self.pending,
                    account,
                )
            {
                summary.skipped.push(id.clone());
                continue;
            }
            self.pending.insert(id.clone(), account.clone());
            self.scan.scanned_ids.insert(id.clone());
            summary.joined.push(id.clone());
        }
        summary
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
        use crate::tui::runtime::SubscriptionPoll;

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

impl<EnvValue, Modal, PendingOpCommit> SettingsAuthSlot
    for SettingsAuthState<EnvValue, Modal, PendingOpCommit>
{
    type Modal = Modal;

    fn modal_mut(&mut self) -> Option<&mut Self::Modal> {
        self.modals.current_mut()
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
> crate::tui::model::ConsoleSettingsModalPresence
    for SettingsState<
        GlobalMountsState<MountRow, MountModal>,
        SettingsEnvState<EnvValue, EnvModal>,
        SettingsAuthState<AuthValue, AuthModal, PendingOpCommit>,
        Trust,
        ErrorPopup,
    >
{
    fn settings_modal_facts(&self) -> crate::tui::model::ConsoleStageModalFacts {
        crate::tui::model::ConsoleStageModalFacts {
            settings_error_popup_open: self.error_popup.is_some(),
            settings_mounts_modal_open: self.mounts.modals.is_open(),
            settings_env_modal_open: self.env.modals.is_open(),
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
> crate::tui::model::ConsoleSettingsFooterHeight
    for SettingsState<
        GlobalMountsState<MountRow, MountModal>,
        SettingsEnvState<EnvValue, EnvModal>,
        SettingsAuthState<AuthValue, AuthModal, PendingOpCommit>,
        Trust,
        ErrorPopup,
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
> crate::tui::debug::ConsoleSettingsDebugFacts
    for SettingsState<
        GlobalMountsState<MountRow, MountModal>,
        SettingsEnvState<EnvValue, EnvModal>,
        SettingsAuthState<AuthValue, AuthModal, PendingOpCommit>,
        Trust,
        ErrorPopup,
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
                .modals
                .current()
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
        crate::tui::screens::settings::update::settings_map_change_count(
            &self.original,
            &self.pending,
        ) + usize::from(self.github != self.original_github)
            + jackin_core::Agent::ALL
                .iter()
                .filter(|agent| self.original_bindings.get(*agent) != self.bindings.get(*agent))
                .count()
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

/// Each provider can own any number of independently named credentials.
pub const ACCOUNT_KINDS: &[AuthKind] = &[
    AuthKind::Claude,
    AuthKind::Codex,
    AuthKind::Amp,
    AuthKind::Kimi,
    AuthKind::Opencode,
    AuthKind::Grok,
    AuthKind::Antigravity,
    AuthKind::Gemini,
    AuthKind::Cursor,
    AuthKind::Muse,
    AuthKind::Omp,
    AuthKind::Hermes,
    AuthKind::Zai,
    AuthKind::Minimax,
];

pub fn account_kind(account: &jackin_config::AccountConfig) -> AuthKind {
    use jackin_config::AiProvider;
    // Owner-bearing credentials resolve by owner, not provider: two agents
    // share the Google provider, so provider-only mapping would rewrite an
    // Antigravity profile's owner on save.
    match &account.credential {
        jackin_config::AccountCredential::Profile { agent, .. }
        | jackin_config::AccountCredential::OAuthToken { agent, .. } => {
            if let Some(kind) = auth_kind_for_agent(*agent) {
                return kind;
            }
        }
        jackin_config::AccountCredential::ApiKey { .. } => {}
    }
    match account.provider {
        AiProvider::Anthropic => AuthKind::Claude,
        AiProvider::OpenAi => AuthKind::Codex,
        AiProvider::Amp => AuthKind::Amp,
        AiProvider::Moonshot => AuthKind::Kimi,
        AiProvider::Opencode => AuthKind::Opencode,
        AiProvider::Xai => AuthKind::Grok,
        // Ownerless Google keys show under Gemini; either Google agent can
        // consume them and the save roundtrips to the same provider.
        AiProvider::Google => AuthKind::Gemini,
        AiProvider::Cursor => AuthKind::Cursor,
        AiProvider::Meta => AuthKind::Muse,
        // Routed-provider accounts show under the first multi-provider kind;
        // the save roundtrips by provider, not kind.
        AiProvider::OpenRouter => AuthKind::Omp,
        AiProvider::Zai => AuthKind::Zai,
        AiProvider::Minimax => AuthKind::Minimax,
    }
}

/// Inverse of [`auth_kind_agent`](crate::tui::auth_config::auth_kind_agent).
fn auth_kind_for_agent(agent: jackin_core::Agent) -> Option<AuthKind> {
    use jackin_core::Agent;
    match agent {
        Agent::Claude => Some(AuthKind::Claude),
        Agent::Codex => Some(AuthKind::Codex),
        Agent::Amp => Some(AuthKind::Amp),
        Agent::Kimi => Some(AuthKind::Kimi),
        Agent::Opencode => Some(AuthKind::Opencode),
        Agent::Grok => Some(AuthKind::Grok),
        Agent::Antigravity => Some(AuthKind::Antigravity),
        Agent::Gemini => Some(AuthKind::Gemini),
        Agent::Cursor => Some(AuthKind::Cursor),
        Agent::Muse => Some(AuthKind::Muse),
        Agent::Omp => Some(AuthKind::Omp),
        Agent::Hermes => Some(AuthKind::Hermes),
    }
}
