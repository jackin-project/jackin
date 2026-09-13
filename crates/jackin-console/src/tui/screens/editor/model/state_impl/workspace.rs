use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    AuthEnterPlan, AuthRow, EditorAuthActionKeyPlan, EditorEnterKeyPlan, EditorEscapeKeyPlan,
    EditorFieldSelectionKeyPlan, EditorImmediateActionKeyPlan, EditorMode,
    EditorMountActionKeyPlan, EditorRoleActionKeyPlan, EditorRoleHeaderExpansionKeyPlan,
    EditorSaveKeyPlan, EditorSecretsActionKeyPlan, EditorState, EditorTab, EditorTabActionKeyPlan,
    FieldFocus, RoleHeaderExpansionPlan, SecretsEnterPlan, SecretsRow, SecretsScopeTag,
};

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>
    EditorState<
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    pub fn commit_workdir_input(&mut self, workdir: impl Into<String>) {
        self.pending.workdir = workdir.into();
        self.clear_modal_chain();
    }

    pub fn commit_last_mount_dst_input(&mut self, dst: impl Into<String>) {
        if let Some(last) = self.pending.mounts.last_mut() {
            last.dst = dst.into();
        }
        self.clear_modal_chain();
    }

    pub fn apply_confirmed_mounts(
        &mut self,
        final_mounts: Option<Vec<jackin_config::MountConfig>>,
    ) {
        if let Some(final_mounts) = final_mounts {
            self.pending.mounts = final_mounts;
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        if self.pending != self.original {
            return true;
        }
        if let EditorMode::Edit { name } = &self.mode
            && self.pending_name.as_deref().is_some_and(|n| n != name)
        {
            return true;
        }
        false
    }

    #[must_use]
    pub fn change_count(&self) -> usize {
        let mut n = 0;
        if self.pending.workdir != self.original.workdir {
            n += 1;
        }
        if self.pending.default_role != self.original.default_role {
            n += 1;
        }
        if self.pending.allowed_roles != self.original.allowed_roles {
            n += 1;
        }
        if self.pending.keep_awake != self.original.keep_awake {
            n += 1;
        }
        if self.pending.git_pull_on_entry != self.original.git_pull_on_entry {
            n += 1;
        }
        if self.pending.github != self.original.github {
            n += 1;
        }
        if self.pending.accounts != self.original.accounts {
            n += 1;
        }
        if self.pending.account_bindings != self.original.account_bindings {
            n += 1;
        }
        if let EditorMode::Edit { name } = &self.mode
            && self.pending_name.as_deref().is_some_and(|pn| pn != name)
        {
            n += 1;
        }
        n += crate::mount_diff::classify_mount_diffs(&self.original.mounts, &self.pending.mounts)
            .iter()
            .filter(|d| !matches!(d, crate::mount_diff::MountDiff::Unchanged(_)))
            .count();
        n += crate::tui::screens::settings::update::settings_map_change_count(
            &self.original.env,
            &self.pending.env,
        );

        let role_keys: BTreeSet<&String> = self
            .original
            .roles
            .keys()
            .chain(self.pending.roles.keys())
            .collect();
        for role in role_keys {
            let orig = self.original.roles.get(role);
            let pend = self.pending.roles.get(role);
            let empty = BTreeMap::<String, jackin_config::EnvValue>::new();
            let orig_env = orig.map_or(&empty, |o| &o.env);
            let pend_env = pend.map_or(&empty, |p| &p.env);
            n += crate::tui::screens::settings::update::settings_map_change_count(
                orig_env, pend_env,
            );
            if orig.and_then(|o| o.github.as_ref()) != pend.and_then(|p| p.github.as_ref()) {
                n += 1;
            }
            let empty_bindings = BTreeMap::new();
            if orig.map_or(&empty_bindings, |o| &o.account_bindings)
                != pend.map_or(&empty_bindings, |p| &p.account_bindings)
            {
                n += 1;
            }
        }
        n
    }

    pub fn cycle_isolation_for_selected_mount(&mut self) {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::cycle_mount_isolation_at(&mut self.pending.mounts, n);
    }

    pub fn remove_selected_mount(&mut self) {
        let FieldFocus::Row(n) = self.active_field;
        if n < self.pending.mounts.len() {
            self.pending.mounts.remove(n);
        }
    }

    pub fn add_shared_mount(&mut self, src: &str, dst: &str) {
        self.pending
            .mounts
            .push(crate::services::workspace::shared_mount_config(
                src, dst, false,
            ));
    }

    pub fn toggle_general_selected(&mut self) {
        let FieldFocus::Row(row) = self.active_field;
        match row {
            2 => {
                self.pending.keep_awake.enabled = !self.pending.keep_awake.enabled;
            }
            3 => {
                self.pending.git_pull_on_entry = !self.pending.git_pull_on_entry;
            }
            _ => {}
        }
    }

    pub fn toggle_selected_mount_readonly(&mut self) {
        let FieldFocus::Row(row) = self.active_field;
        if let Some(mount) = self.pending.mounts.get_mut(row) {
            mount.readonly = !mount.readonly;
        }
    }

    #[must_use]
    pub fn eligible_role_override_selectors<'a>(
        &self,
        registered_roles: impl Iterator<Item = &'a String> + 'a,
    ) -> Vec<jackin_core::RoleSelector> {
        crate::workspace::eligible_role_keys_for_override(registered_roles, &self.pending)
            .into_iter()
            .filter_map(|name| jackin_core::RoleSelector::parse(&name).ok())
            .collect()
    }

    pub fn toggle_allowed_role_at_cursor(&mut self, role_names: &[String]) {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::toggle_allowed_role_at(
            &mut self.pending.allowed_roles,
            &mut self.pending.default_role,
            role_names,
            n,
        );
    }

    pub fn toggle_default_role_at_cursor(&mut self, role_names: &[String]) {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::toggle_default_role_at(
            &self.pending.allowed_roles,
            &mut self.pending.default_role,
            role_names,
            n,
        );
    }

    pub fn set_secrets_role_expanded(&mut self, role: String, expanded: bool) {
        if expanded {
            self.secrets_expanded.insert(role);
        } else {
            self.secrets_expanded.remove(&role);
        }
    }

    pub fn toggle_secret_mask(&mut self, scope: SecretsScopeTag, key: String) {
        let entry = (scope, key);
        if !self.unmasked_rows.remove(&entry) {
            self.unmasked_rows.insert(entry);
        }
    }

    /// Delete an environment key from the draft workspace or role override.
    /// Preserve a role override while it still owns account or GitHub bindings.
    pub fn delete_env_var(&mut self, scope: &SecretsScopeTag, key: &str) -> anyhow::Result<()> {
        match scope {
            SecretsScopeTag::Workspace => {
                self.pending.env.remove(key);
            }
            SecretsScopeTag::Role(role) => {
                let mut drop_role = false;
                if let Some(override_config) = self.pending.roles.get_mut(role) {
                    override_config.env.remove(key);
                    drop_role = override_config.env.is_empty()
                        && override_config.account_bindings.is_empty();
                }
                if drop_role {
                    self.pending.roles.remove(role);
                }
            }
        }

        Ok(())
    }

    /// Toggle assignment or cycle an explicit binding through compatible assigned accounts.
    pub fn edit_account_row(&mut self, config: &jackin_config::AppConfig, clear: bool) {
        let FieldFocus::Row(index) = self.active_field;
        let rows = self.auth_flat_rows(config);
        match rows.get(index) {
            Some(AuthRow::Account { id }) => {
                if clear || self.pending.accounts.contains(id) {
                    self.pending.accounts.retain(|value| value != id);
                    self.pending.account_bindings.retain(|_, value| value != id);
                    for role in self.pending.roles.values_mut() {
                        role.account_bindings.retain(|_, value| value != id);
                    }
                } else if config
                    .accounts
                    .get(id)
                    .is_some_and(|account| account.enabled)
                {
                    self.pending.accounts.push(id.clone());
                }
            }
            Some(AuthRow::Binding { agent, role }) => {
                let candidates: Vec<_> = self
                    .pending
                    .accounts
                    .iter()
                    .filter(|id| {
                        config
                            .accounts
                            .get(*id)
                            .is_some_and(|account| account.supports_agent(*agent))
                    })
                    .cloned()
                    .collect();
                let bindings = match role {
                    Some(role) => {
                        &mut self
                            .pending
                            .roles
                            .entry(role.clone())
                            .or_default()
                            .account_bindings
                    }
                    None => &mut self.pending.account_bindings,
                };
                let next = if clear {
                    None
                } else {
                    match bindings.get(agent) {
                        Some(current) => candidates
                            .iter()
                            .position(|id| id == current)
                            .and_then(|index| candidates.get(index + 1))
                            .cloned(),
                        None => candidates.first().cloned(),
                    }
                };
                if let Some(id) = next {
                    bindings.insert(*agent, id);
                } else {
                    bindings.remove(agent);
                }
            }
            _ => {}
        }
    }

    #[must_use]
    pub fn focused_account_row(&self, config: &jackin_config::AppConfig) -> bool {
        let FieldFocus::Row(index) = self.active_field;
        matches!(
            self.auth_flat_rows(config).get(index),
            Some(AuthRow::Account { .. } | AuthRow::Binding { .. })
        )
    }

    #[must_use]
    pub fn focused_auth_form(
        &self,
        config: &jackin_config::AppConfig,
    ) -> Option<(
        crate::tui::state::AuthFormTarget,
        crate::tui::state::AuthForm,
    )> {
        let FieldFocus::Row(index) = self.active_field;
        let target = self.resolve_auth_form_target(config, index)?;
        if *target.kind() != crate::tui::auth::AuthKind::Github {
            return None;
        }
        let existing = match &target {
            crate::tui::state::AuthFormTarget::Workspace { .. } => self.pending.github.as_ref(),
            crate::tui::state::AuthFormTarget::WorkspaceRole { role, .. } => self
                .pending
                .roles
                .get(role)
                .and_then(|role| role.github.as_ref()),
        };
        let form = existing.map_or_else(
            || crate::tui::state::AuthForm::new(crate::tui::auth::AuthKind::Github),
            |github| {
                let mode = match github.auth_forward {
                    jackin_config::GithubAuthMode::Sync => crate::tui::auth::AuthMode::Sync,
                    jackin_config::GithubAuthMode::Token => crate::tui::auth::AuthMode::Token,
                    jackin_config::GithubAuthMode::Ignore => crate::tui::auth::AuthMode::Ignore,
                };
                crate::tui::state::AuthForm::from_existing(
                    crate::tui::auth::AuthKind::Github,
                    mode,
                    github.env.get("GH_TOKEN").cloned(),
                )
            },
        );
        Some((target, form))
    }

    pub fn persist_auth_form(
        &mut self,
        target: &crate::tui::state::AuthFormTarget,
        form: &crate::tui::state::AuthForm,
    ) {
        if *target.kind() != crate::tui::auth::AuthKind::Github {
            return;
        }
        let Some(outcome) = form.commit() else {
            return;
        };
        let mode = match outcome.mode {
            crate::tui::auth::AuthMode::Sync => jackin_config::GithubAuthMode::Sync,
            crate::tui::auth::AuthMode::Token => jackin_config::GithubAuthMode::Token,
            crate::tui::auth::AuthMode::Ignore => jackin_config::GithubAuthMode::Ignore,
            _ => return,
        };
        let slot = match target {
            crate::tui::state::AuthFormTarget::Workspace { .. } => &mut self.pending.github,
            crate::tui::state::AuthFormTarget::WorkspaceRole { role, .. } => {
                &mut self.pending.roles.entry(role.clone()).or_default().github
            }
        };
        let github = slot.get_or_insert_with(Default::default);
        github.auth_forward = mode;
        github.env.remove("GH_TOKEN");
        if let Some(value) = outcome.env_value {
            github.env.insert("GH_TOKEN".into(), value);
        }
    }

    pub fn clear_auth_form_layer(&mut self, target: &crate::tui::state::AuthFormTarget) {
        if *target.kind() != crate::tui::auth::AuthKind::Github {
            return;
        }
        match target {
            crate::tui::state::AuthFormTarget::Workspace { .. } => self.pending.github = None,
            crate::tui::state::AuthFormTarget::WorkspaceRole { role, .. } => {
                if let Some(role) = self.pending.roles.get_mut(role) {
                    role.github = None;
                }
            }
        }
    }

    pub fn clear_auth_row_at_cursor(&mut self, config: &jackin_config::AppConfig) {
        if self.focused_account_row(config) {
            self.edit_account_row(config, true);
        } else {
            let FieldFocus::Row(index) = self.active_field;
            if let Some(target) = self.resolve_auth_form_target(config, index) {
                self.clear_auth_form_layer(&target);
            }
        }
    }

    #[must_use]
    pub fn secret_value(
        &self,
        scope: &SecretsScopeTag,
        key: &str,
    ) -> Option<&jackin_core::EnvValue> {
        match scope {
            SecretsScopeTag::Workspace => self.pending.env.get(key),
            SecretsScopeTag::Role(role) => self
                .pending
                .roles
                .get(role)
                .and_then(|role_override| role_override.env.get(key)),
        }
    }

    #[must_use]
    pub fn secret_is_text_editable(&self, scope: &SecretsScopeTag, key: &str) -> bool {
        !self
            .secret_value(scope, key)
            .is_some_and(|value| matches!(value, jackin_core::EnvValue::OpRef(_)))
    }

    #[must_use]
    pub fn focused_secret_is_op_ref(&self) -> bool {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        match rows.get(n) {
            Some(SecretsRow::WorkspaceKeyRow(key)) => self
                .pending
                .env
                .get(key)
                .is_some_and(|value| matches!(value, jackin_core::EnvValue::OpRef(_))),
            Some(SecretsRow::RoleKeyRow { role, key }) => self
                .pending
                .roles
                .get(role)
                .and_then(|role_override| role_override.env.get(key))
                .is_some_and(|value| matches!(value, jackin_core::EnvValue::OpRef(_))),
            _ => false,
        }
    }

    /// No-op on header/sentinel/op:// rows.
    #[must_use]
    pub fn focused_unmask_key(&self) -> Option<(SecretsScopeTag, String)> {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        crate::tui::screens::editor::update::secret_unmask_target_for_row(
            rows.get(n),
            |scope, key| self.secret_is_text_editable(scope, key),
        )
    }

    #[must_use]
    pub fn focused_secret_enter_plan(&self) -> SecretsEnterPlan {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        crate::tui::screens::editor::update::secret_enter_plan_for_row(rows.get(n), |scope, key| {
            self.secret_is_text_editable(scope, key)
        })
    }

    #[must_use]
    pub fn focused_secret_delete_target(&self) -> Option<(SecretsScopeTag, String)> {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        crate::tui::screens::editor::update::secret_delete_target_for_row(rows.get(n))
    }

    #[must_use]
    pub fn focused_secret_add_target(&self) -> Option<SecretsScopeTag> {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        crate::tui::screens::editor::update::secret_add_target_for_row(rows.get(n))
    }

    #[must_use]
    pub fn focused_secrets_role_expansion_plan(&self, expanded: bool) -> RoleHeaderExpansionPlan {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        let Some(SecretsRow::RoleHeader {
            role,
            expanded: current,
        }) = rows.get(n).cloned()
        else {
            return RoleHeaderExpansionPlan::NotHeader;
        };
        if current == expanded {
            RoleHeaderExpansionPlan::HeaderNoop
        } else {
            RoleHeaderExpansionPlan::Set { role, expanded }
        }
    }

    #[must_use]
    pub fn synthesize_app_config_for_auth(
        &self,
        config: &jackin_config::AppConfig,
    ) -> jackin_config::AppConfig {
        let mut result = config.clone();
        result
            .workspaces
            .insert(self.workspace_name_for_panel(), self.pending.clone());
        result
    }

    #[must_use]
    pub fn secrets_flat_rows(&self) -> Vec<SecretsRow> {
        crate::tui::screens::editor::update::secrets_flat_rows(
            &self.pending.env,
            &self.pending.roles,
            &self.secrets_expanded,
            |role| &role.env,
        )
    }

    #[must_use]
    pub fn auth_flat_rows(
        &self,
        config: &jackin_config::AppConfig,
    ) -> Vec<AuthRow<crate::tui::auth::AuthKind>> {
        let mut rows: Vec<_> = config
            .accounts
            .keys()
            .map(|id| AuthRow::Account { id: id.clone() })
            .collect();
        for &agent in jackin_core::Agent::ALL {
            rows.push(AuthRow::Binding { agent, role: None });
        }
        rows.push(AuthRow::WorkspaceMode {
            kind: crate::tui::auth::AuthKind::Github,
        });
        for role in self.eligible_role_override_selectors(config.roles.keys()) {
            rows.push(AuthRow::RoleMode {
                role: role.key(),
                kind: crate::tui::auth::AuthKind::Github,
            });
            for &agent in jackin_core::Agent::ALL {
                rows.push(AuthRow::Binding {
                    agent,
                    role: Some(role.key()),
                });
            }
        }
        rows
    }

    #[must_use]
    pub fn focused_auth_enter_plan(&self, config: &jackin_config::AppConfig) -> AuthEnterPlan {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.auth_flat_rows(config);
        match rows.get(n) {
            Some(AuthRow::WorkspaceMode { .. } | AuthRow::RoleMode { .. }) => {
                AuthEnterPlan::OpenForm
            }
            _ => AuthEnterPlan::Noop,
        }
    }

    #[must_use]
    pub fn enter_key_plan(
        &self,
        config: &jackin_config::AppConfig,
        op_available: bool,
    ) -> EditorEnterKeyPlan {
        match self.active_tab {
            EditorTab::General => EditorEnterKeyPlan::OpenGeneralField,
            EditorTab::Mounts if self.focused_mount_add_row_selected() => {
                EditorEnterKeyPlan::OpenMountFileBrowser
            }
            EditorTab::Mounts => EditorEnterKeyPlan::Noop,
            EditorTab::Secrets if self.focused_secret_is_op_ref() && op_available => {
                EditorEnterKeyPlan::OpenSecretsPicker
            }
            EditorTab::Secrets => EditorEnterKeyPlan::OpenSecretsEnterModal,
            EditorTab::Roles if self.focused_role_add_row_selected(config) => {
                EditorEnterKeyPlan::OpenRoleInput
            }
            EditorTab::Roles => EditorEnterKeyPlan::Noop,
            EditorTab::Auth => EditorEnterKeyPlan::Auth(self.focused_auth_enter_plan(config)),
        }
    }

    #[must_use]
    pub fn escape_key_plan(&self) -> EditorEscapeKeyPlan {
        if !self.tab_bar_focused() {
            return EditorEscapeKeyPlan::FocusTabBar;
        }
        use crate::tui::screens::edit_save::{EditSaveDisposition, plan_leave_when_dirty};
        match plan_leave_when_dirty(self.is_dirty()) {
            EditSaveDisposition::ConfirmDiscard => EditorEscapeKeyPlan::OpenSaveDiscard,
            EditSaveDisposition::Noop | EditSaveDisposition::SaveNow => {
                EditorEscapeKeyPlan::ReloadFromConfig
            }
        }
    }

    #[must_use]
    pub fn save_key_plan(&self) -> EditorSaveKeyPlan {
        use crate::tui::screens::edit_save::{EditSaveDisposition, plan_explicit_save};
        match plan_explicit_save(self.change_count() > 0) {
            EditSaveDisposition::Noop => EditorSaveKeyPlan::Noop,
            EditSaveDisposition::SaveNow | EditSaveDisposition::ConfirmDiscard => {
                EditorSaveKeyPlan::BeginSave
            }
        }
    }

    #[must_use]
    pub fn focused_role_header_expansion_key_plan(
        &self,
        _config: &jackin_config::AppConfig,
        expanded: bool,
    ) -> EditorRoleHeaderExpansionKeyPlan {
        match self.active_tab {
            EditorTab::Secrets => EditorRoleHeaderExpansionKeyPlan::Secrets(
                self.focused_secrets_role_expansion_plan(expanded),
            ),
            EditorTab::Auth | EditorTab::General | EditorTab::Mounts | EditorTab::Roles => {
                EditorRoleHeaderExpansionKeyPlan::NotRoleHeaderTab
            }
        }
    }

    #[must_use]
    pub fn focused_mount_add_row_selected(&self) -> bool {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::editor_mount_add_row_selected(
            n,
            self.pending.mounts.len(),
        )
    }

    #[must_use]
    pub fn focused_role_add_row_selected(&self, config: &jackin_config::AppConfig) -> bool {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::editor_role_add_row_selected(n, config.roles.len())
    }

    #[must_use]
    pub fn selection_bounds(&self, config: &jackin_config::AppConfig) -> (usize, Vec<usize>) {
        let secrets_rows = self.secrets_flat_rows();
        let auth_rows = self.auth_flat_rows(config);
        crate::tui::screens::editor::update::editor_selection_bounds(
            self.active_tab,
            self.pending.mounts.len(),
            config.roles.len(),
            &secrets_rows,
            &auth_rows,
        )
    }

    #[must_use]
    pub fn field_selection_key_plan(
        &self,
        config: &jackin_config::AppConfig,
        delta: isize,
        term: ratatui::layout::Rect,
    ) -> EditorFieldSelectionKeyPlan {
        let (max_row, skipped_rows) = self.selection_bounds(config);
        EditorFieldSelectionKeyPlan {
            delta,
            max_row,
            skipped_rows,
            term,
            footer_h: self.cached_footer_h,
        }
    }

    #[must_use]
    pub fn immediate_action_key_plan(
        &self,
        _config: &jackin_config::AppConfig,
        key_code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) -> EditorImmediateActionKeyPlan {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key_code {
            KeyCode::Char(' ') if self.active_tab == EditorTab::General => {
                EditorImmediateActionKeyPlan::ToggleGeneralSelected
            }
            KeyCode::Char('r' | 'R') if self.active_tab == EditorTab::Mounts => {
                EditorImmediateActionKeyPlan::ToggleMountReadonlySelected
            }
            KeyCode::Char('m' | 'M')
                if self.active_tab == EditorTab::Secrets
                    && (modifiers - KeyModifiers::SHIFT).is_empty() =>
            {
                self.focused_unmask_key().map_or(
                    EditorImmediateActionKeyPlan::NotImmediateAction,
                    |(scope, key)| EditorImmediateActionKeyPlan::ToggleSecretMask { scope, key },
                )
            }
            _ => EditorImmediateActionKeyPlan::NotImmediateAction,
        }
    }

    #[must_use]
    pub fn role_action_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
    ) -> EditorRoleActionKeyPlan {
        use crossterm::event::KeyCode;

        if self.active_tab != EditorTab::Roles {
            return EditorRoleActionKeyPlan::NotRoleAction;
        }

        match key_code {
            KeyCode::Char('a' | 'A') => EditorRoleActionKeyPlan::OpenRoleInput,
            KeyCode::Char(' ') => EditorRoleActionKeyPlan::ToggleAllowed,
            KeyCode::Char('*') => EditorRoleActionKeyPlan::ToggleDefault,
            _ => EditorRoleActionKeyPlan::NotRoleAction,
        }
    }

    #[must_use]
    pub fn mount_action_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
    ) -> EditorMountActionKeyPlan {
        use crossterm::event::KeyCode;

        if self.active_tab != EditorTab::Mounts {
            return EditorMountActionKeyPlan::NotMountAction;
        }

        match key_code {
            KeyCode::Char('a' | 'A') => EditorMountActionKeyPlan::AddMount,
            KeyCode::Char('d' | 'D') => EditorMountActionKeyPlan::RemoveSelectedMount,
            KeyCode::Char('i' | 'I') => EditorMountActionKeyPlan::CycleIsolation,
            KeyCode::Char('o' | 'O') => EditorMountActionKeyPlan::OpenGithub,
            _ => EditorMountActionKeyPlan::NotMountAction,
        }
    }

    #[must_use]
    pub fn secrets_action_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
        op_available: bool,
    ) -> EditorSecretsActionKeyPlan {
        use crossterm::event::{KeyCode, KeyModifiers};

        if self.active_tab != EditorTab::Secrets || !(modifiers - KeyModifiers::SHIFT).is_empty() {
            return EditorSecretsActionKeyPlan::NotSecretsAction;
        }

        match key_code {
            KeyCode::Char('p' | 'P') if op_available => EditorSecretsActionKeyPlan::OpenPicker,
            KeyCode::Char('d' | 'D') => EditorSecretsActionKeyPlan::OpenDeleteConfirm,
            KeyCode::Char('a' | 'A') => EditorSecretsActionKeyPlan::OpenAddModal,
            _ => EditorSecretsActionKeyPlan::NotSecretsAction,
        }
    }

    #[must_use]
    pub fn auth_action_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
    ) -> EditorAuthActionKeyPlan {
        use crossterm::event::KeyCode;

        if self.active_tab != EditorTab::Auth {
            return EditorAuthActionKeyPlan::NotAuthAction;
        }

        match key_code {
            KeyCode::Char('d' | 'D') => EditorAuthActionKeyPlan::ClearFocusedRow,
            _ => EditorAuthActionKeyPlan::NotAuthAction,
        }
    }

    #[must_use]
    pub fn tab_action_key_plan(
        &self,
        config: &jackin_config::AppConfig,
        key_code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
        op_available: bool,
    ) -> EditorTabActionKeyPlan {
        use crossterm::event::KeyCode;

        let role_action = self.role_action_key_plan(key_code);
        if !matches!(role_action, EditorRoleActionKeyPlan::NotRoleAction) {
            return EditorTabActionKeyPlan::Role(role_action);
        }

        let mount_action = self.mount_action_key_plan(key_code);
        if !matches!(mount_action, EditorMountActionKeyPlan::NotMountAction) {
            return EditorTabActionKeyPlan::Mount(mount_action);
        }

        let secrets_action = self.secrets_action_key_plan(key_code, modifiers, op_available);
        if !matches!(secrets_action, EditorSecretsActionKeyPlan::NotSecretsAction) {
            return EditorTabActionKeyPlan::Secrets(secrets_action);
        }

        let auth_action = self.auth_action_key_plan(key_code);
        if !matches!(auth_action, EditorAuthActionKeyPlan::NotAuthAction) {
            return EditorTabActionKeyPlan::Auth(auth_action);
        }

        if key_code == KeyCode::Enter {
            return EditorTabActionKeyPlan::Enter(self.enter_key_plan(config, op_available));
        }

        EditorTabActionKeyPlan::Noop
    }

    #[must_use]
    pub fn resolve_auth_form_target(
        &self,
        config: &jackin_config::AppConfig,
        row: usize,
    ) -> Option<crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>>
    {
        let rows = self.auth_flat_rows(config);
        crate::tui::screens::editor::update::resolve_auth_form_target(&rows, row)
    }
}
