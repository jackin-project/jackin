/// `SettingsEnvState` impls + `settings_env_config_from_app_config` helper.
use super::{
    BTreeMap, SettingsEnvConfig, SettingsEnvScope, SettingsModalSlot, SettingsPanelChangeCount,
    SettingsPanelDirty, SettingsPanelDiscard, SettingsPanelMarkSaved, SettingsPanelTakeError,
};

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
