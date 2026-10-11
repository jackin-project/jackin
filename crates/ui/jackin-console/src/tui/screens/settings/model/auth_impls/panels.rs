// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `SettingsAuthState` settings-panel trait impls.

use super::super::{
    GlobalMountsState, SettingsAuthRestorePendingForm, SettingsAuthSlot, SettingsAuthState,
    SettingsEnvState, SettingsPanelChangeCount, SettingsPanelDirty, SettingsPanelDiscard,
    SettingsPanelMarkSaved, SettingsPanelTakeError, SettingsState,
};

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
