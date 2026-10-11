// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `SettingsAuthState` inherent methods.

use super::super::{
    AccountScanOutcome, AccountScanState, AccountScanSummary, AuthKind, BTreeMap,
    SettingsAuthSaveRefs, SettingsAuthState,
};
use super::{ACCOUNT_KINDS, account_kind};
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
