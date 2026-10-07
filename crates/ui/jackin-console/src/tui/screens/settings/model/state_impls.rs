// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings aggregate state impls.

use super::{
    GlobalMountsState, SettingsAfterEventOutcome, SettingsAuthRestorePendingForm, SettingsAuthSlot,
    SettingsAuthState, SettingsEnvRow, SettingsEnvState, SettingsGeneralState, SettingsHoverTarget,
    SettingsModalSlot, SettingsMountsTakeExit, SettingsPanelChangeCount, SettingsPanelDirty,
    SettingsPanelDiscard, SettingsPanelMarkSaved, SettingsPanelTakeError, SettingsState,
    SettingsTab, SettingsTrustState,
};

use crate::tui::focus::{ConsoleFocusTarget, TabFocus};

impl<Mounts, Env, Auth, Trust, ErrorPopup> SettingsState<Mounts, Env, Auth, Trust, ErrorPopup> {
    pub fn dismiss_error_popup(&mut self)
    where
        Auth: SettingsAuthRestorePendingForm,
    {
        self.error_popup = None;
        self.auth.restore_pending_auth_form();
    }

    #[must_use]
    pub fn focus_owner(&self) -> ConsoleFocusTarget<SettingsTab> {
        self.focus_owner.focused()
    }

    #[must_use]
    pub const fn content_area(&self, term_size: ratatui::layout::Rect) -> ratatui::layout::Rect {
        crate::tui::layout::tabbed_content_area(term_size, self.cached_footer_h)
    }

    pub fn set_focus_owner(&mut self, owner: ConsoleFocusTarget<SettingsTab>) {
        match owner {
            ConsoleFocusTarget::TabBar => self.focus_owner.focus_tab_bar(),
            ConsoleFocusTarget::Content(tab) => self.focus_owner.focus_content(tab),
        }
    }

    pub fn apply_tab_move_plan(
        &mut self,
        plan: crate::tui::screens::settings::update::SettingsTabMovePlan,
    ) {
        self.active_tab = plan.active_tab;
        self.set_tab_bar_focused(plan.tab_bar_focused);
    }

    #[must_use]
    pub fn tab_bar_focused(&self) -> bool {
        self.focus_owner.is_tab_bar()
    }

    pub fn set_tab_bar_focused(&mut self, focused: bool) {
        if focused {
            self.focus_owner.focus_tab_bar();
        } else {
            self.focus_owner.focus_content(self.active_tab);
        }
    }

    pub fn apply_tab_bar_focus_plan(&mut self, focused: bool) {
        self.set_tab_bar_focused(focused);
    }

    #[must_use]
    pub fn content_focused(&self, tab: SettingsTab) -> bool {
        self.focus_owner.is_content(tab)
    }

    pub fn set_content_focused(&mut self, tab: SettingsTab, focused: bool) {
        if focused {
            self.focus_owner.focus_content(tab);
        } else if self.content_focused(tab) {
            self.focus_owner.focus_tab_bar();
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

impl<Mounts, Env, Auth, Trust>
    SettingsState<Mounts, Env, Auth, Trust, crate::tui::components::ErrorPopupState>
{
    pub fn open_error_popup(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.error_popup = Some(crate::tui::components::error_popup::error_popup_state(
            title, message,
        ));
    }
}

impl<Mounts, Env, Auth, Trust, ErrorPopup> crate::tui::model::ConsolePendingOpCommit
    for SettingsState<Mounts, Env, Auth, Trust, ErrorPopup>
where
    Auth: crate::tui::model::ConsolePendingOpCommit,
{
    type OpRef = Auth::OpRef;

    fn poll_pending_op_commit(&mut self) -> Option<(Self::OpRef, anyhow::Result<()>)> {
        self.auth.poll_pending_op_commit()
    }
}

impl<Mounts, Env, Auth, Trust, ErrorPopup> crate::tui::model::ConsoleAnimationTick
    for SettingsState<Mounts, Env, Auth, Trust, ErrorPopup>
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

impl<Mounts, Env, Auth, Trust, ErrorPopup> SettingsState<Mounts, Env, Auth, Trust, ErrorPopup>
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

impl<Mounts, EnvValue, EnvModal, Auth, Trust, ErrorPopup>
    SettingsState<Mounts, SettingsEnvState<EnvValue, EnvModal>, Auth, Trust, ErrorPopup>
{
    #[must_use]
    pub fn env_flat_rows(&self) -> Vec<SettingsEnvRow> {
        crate::tui::screens::settings::update::settings_env_flat_rows(
            &self.env.pending,
            &self.env.expanded,
        )
    }
}

impl<MountModal, EnvModal, AuthModal, PendingOpCommit, ErrorPopup>
    SettingsState<
        GlobalMountsState<jackin_config::GlobalMountRow, MountModal>,
        SettingsEnvState<jackin_config::EnvValue, EnvModal>,
        SettingsAuthState<jackin_config::EnvValue, AuthModal, PendingOpCommit>,
        SettingsTrustState,
        ErrorPopup,
    >
{
    #[must_use]
    pub fn from_config(config: &jackin_config::AppConfig) -> Self {
        Self {
            active_tab: SettingsTab::General,
            focus_owner: TabFocus::tab_bar(SettingsTab::General),
            hover_target: None,
            general: SettingsGeneralState::from_values(config.git.coauthor_trailer, config.git.dco),
            mounts: GlobalMountsState::from_rows(config.list_mount_rows()),
            env: SettingsEnvState::from_config(config),
            auth: SettingsAuthState::from_config(config),
            trust: SettingsTrustState::from_config(config),
            error_popup: None,

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
            &mut self.mounts.scroll,
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
        self.auth.row_count()
            + usize::from(self.auth.error.is_some())
            + self.auth.scan.status_line_count()
    }

    #[must_use]
    pub fn trust_content_height(&self) -> usize {
        crate::tui::screens::settings::view::trust_content_height(
            self.trust.pending.len(),
            self.trust.error.is_some(),
        )
    }
}
