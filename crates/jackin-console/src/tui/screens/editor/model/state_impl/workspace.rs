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
    AuthFormTarget,
    PendingTokenGenerate,
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
        AuthFormTarget,
        PendingTokenGenerate,
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
        if self.pending.claude != self.original.claude {
            n += 1;
        }
        if self.pending.codex != self.original.codex {
            n += 1;
        }
        if self.pending.github != self.original.github {
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
            if orig.map(|o| &o.claude) != pend.map(|p| &p.claude) {
                n += 1;
            }
            if orig.map(|o| &o.codex) != pend.map(|p| &p.codex) {
                n += 1;
            }
            if orig.map(|o| &o.github) != pend.map(|p| &p.github) {
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

    #[must_use]
    pub fn auth_role_override_selectors<'a>(
        &self,
        registered_roles: impl Iterator<Item = &'a String> + 'a,
    ) -> Option<Vec<jackin_core::RoleSelector>> {
        let kind = self.auth_selected_kind?;
        let already_overridden: BTreeSet<String> = self
            .pending
            .roles
            .iter()
            .filter(|(_, role_override)| {
                crate::tui::auth_config::role_override_present(kind, role_override)
            })
            .map(|(name, _)| name.clone())
            .collect();

        let candidates =
            crate::workspace::eligible_role_keys_for_override(registered_roles, &self.pending)
                .into_iter()
                .filter(|role| !already_overridden.contains(role))
                .filter_map(|role| jackin_core::RoleSelector::parse(&role).ok())
                .collect();
        Some(candidates)
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

    pub fn toggle_auth_role_expanded(&mut self, role: String) {
        if !self.auth_expanded.remove(&role) {
            self.auth_expanded.insert(role);
        }
    }

    pub fn set_auth_role_expanded(&mut self, role: String, expanded: bool) {
        if expanded {
            self.auth_expanded.insert(role);
        } else {
            self.auth_expanded.remove(&role);
        }
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
    ///
    /// Claude OAuth-token mode owns its token through the token-setup flow, so
    /// the editor must not silently remove that managed slot.
    pub fn delete_env_var(&mut self, scope: &SecretsScopeTag, key: &str) -> anyhow::Result<()> {
        let protected = key == jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME
            && matches!(scope, SecretsScopeTag::Workspace)
            && self.pending.claude.as_ref().map(|c| c.auth_forward)
                == Some(jackin_config::AuthForwardMode::OAuthToken);
        if protected {
            anyhow::bail!(
                "CLAUDE_CODE_OAUTH_TOKEN is managed by `jackin workspace claude-token` \
                 — use `jackin workspace claude-token revoke <workspace>` to clear it"
            );
        }

        match scope {
            SecretsScopeTag::Workspace => {
                self.pending.env.remove(key);
            }
            SecretsScopeTag::Role(role) => {
                let mut drop_role = false;
                if let Some(override_config) = self.pending.roles.get_mut(role) {
                    override_config.env.remove(key);
                    drop_role = override_config.env.is_empty();
                }
                if drop_role {
                    self.pending.roles.remove(role);
                }
            }
        }

        Ok(())
    }

    #[must_use]
    pub fn focused_auth_role_expansion_plan(
        &self,
        config: &jackin_config::AppConfig,
        expanded: bool,
    ) -> RoleHeaderExpansionPlan {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.auth_flat_rows(config);
        let Some(AuthRow::RoleHeader {
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

    pub fn clear_auth_row_at_cursor(&mut self, config: &jackin_config::AppConfig) {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.auth_flat_rows(config);
        match rows.get(n).cloned() {
            Some(AuthRow::RoleHeader { role, .. }) => {
                if let Some(kind) = self.auth_selected_kind {
                    self.clear_role_auth_kind(&role, kind);
                }
            }
            Some(AuthRow::RoleMode { role, kind }) => {
                self.clear_role_auth_kind(&role, kind);
            }
            Some(AuthRow::WorkspaceMode { kind }) => {
                crate::tui::auth_config::clear_workspace_auth_layer(&mut self.pending, kind);
            }
            _ => {}
        }
    }

    #[must_use]
    pub fn focused_auth_form(
        &self,
        config: &jackin_config::AppConfig,
    ) -> Option<(
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
    )> {
        let FieldFocus::Row(n) = self.active_field;
        let target = self.resolve_auth_form_target(config, n)?;
        let kind = *target.kind();
        let (existing_mode, existing_credential) = self.auth_form_mode_and_credential(&target);
        let form = existing_mode
            .map_or_else(
                || crate::tui::components::auth_panel::AuthForm::new(kind),
                |mode| {
                    crate::tui::components::auth_panel::AuthForm::from_existing(
                        kind,
                        mode,
                        existing_credential,
                    )
                },
            )
            .with_source_folder(
                self.auth_form_source_folder(&target),
                self.auth_form_source_folder_fallback(config, &target),
            );
        Some((target, form))
    }

    /// Apply a successful auth-form commit to the draft workspace config.
    ///
    /// Writes both the kind block (`auth_forward`) and the credential env var
    /// when the form outcome includes one.
    pub fn persist_auth_form(
        &mut self,
        target: &crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        form: &crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
    ) {
        let Some(outcome) = form.commit() else {
            return;
        };
        match target {
            crate::tui::screens::settings::model::AuthFormTarget::Workspace { kind } => {
                crate::tui::auth_config::apply_workspace_auth_commit(
                    &mut self.pending,
                    *kind,
                    outcome.mode,
                    outcome.env_var_name,
                    outcome.env_value.clone(),
                );
                crate::tui::auth_config::set_workspace_sync_source_dir(
                    &mut self.pending,
                    *kind,
                    outcome.source_folder,
                );
            }
            crate::tui::screens::settings::model::AuthFormTarget::WorkspaceRole { role, kind } => {
                let entry = self.pending.roles.entry(role.clone()).or_default();
                crate::tui::auth_config::apply_role_auth_commit(
                    entry,
                    *kind,
                    outcome.mode,
                    outcome.env_var_name,
                    outcome.env_value.clone(),
                );
                crate::tui::auth_config::set_role_sync_source_dir(
                    entry,
                    *kind,
                    outcome.source_folder,
                );
            }
        }
    }

    /// Clear the auth layer and source-folder override for the form target.
    pub fn clear_auth_form_layer(
        &mut self,
        target: &crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
    ) {
        match target {
            crate::tui::screens::settings::model::AuthFormTarget::Workspace { kind } => {
                crate::tui::auth_config::clear_workspace_auth_layer(&mut self.pending, *kind);
                crate::tui::auth_config::set_workspace_sync_source_dir(
                    &mut self.pending,
                    *kind,
                    None,
                );
            }
            crate::tui::screens::settings::model::AuthFormTarget::WorkspaceRole { role, kind } => {
                if let Some(entry) = self.pending.roles.get_mut(role) {
                    crate::tui::auth_config::clear_role_auth_layer(entry, *kind);
                    crate::tui::auth_config::set_role_sync_source_dir(entry, *kind, None);
                }
            }
        }
    }

    fn auth_form_mode_and_credential(
        &self,
        target: &crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
    ) -> (
        Option<crate::tui::auth::AuthMode>,
        Option<jackin_core::EnvValue>,
    ) {
        match target {
            crate::tui::screens::settings::model::AuthFormTarget::Workspace { kind } => {
                crate::tui::auth_config::workspace_auth_mode_and_credential(&self.pending, *kind)
            }
            crate::tui::screens::settings::model::AuthFormTarget::WorkspaceRole { role, kind } => {
                crate::tui::auth_config::role_auth_mode_and_credential(
                    self.pending.roles.get(role),
                    *kind,
                )
            }
        }
    }

    fn auth_form_source_folder(
        &self,
        target: &crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
    ) -> Option<std::path::PathBuf> {
        let agent = crate::tui::auth_config::auth_kind_agent(*target.kind())?;
        match target {
            crate::tui::screens::settings::model::AuthFormTarget::Workspace { .. } => {
                self.pending.sync_source_dir_for(agent)
            }
            crate::tui::screens::settings::model::AuthFormTarget::WorkspaceRole {
                role, ..
            } => self
                .pending
                .roles
                .get(role)
                .and_then(|role| role.sync_source_dir_for(agent)),
        }
    }

    fn auth_form_source_folder_fallback(
        &self,
        config: &jackin_config::AppConfig,
        target: &crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
    ) -> Option<crate::tui::components::editor_rows::AuthSourceFolderDisplay> {
        crate::tui::auth_config::auth_kind_agent(*target.kind())?;
        let synthesized = self.synthesize_app_config_for_auth(config);
        let workspace_name = self.workspace_name_for_panel();
        let role = match target {
            crate::tui::screens::settings::model::AuthFormTarget::Workspace { .. } => "",
            crate::tui::screens::settings::model::AuthFormTarget::WorkspaceRole {
                role, ..
            } => role.as_str(),
        };
        Some(crate::tui::auth_config::editor_source_folder_display(
            &synthesized,
            &workspace_name,
            role,
            *target.kind(),
        ))
    }

    fn clear_role_auth_kind(&mut self, role: &str, kind: crate::tui::auth::AuthKind) {
        if let Some(role_override) = self.pending.roles.get_mut(role) {
            crate::tui::auth_config::clear_role_auth_layer(role_override, kind);
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
        crate::tui::auth_config::synthesize_app_config_for_workspace_auth(
            config,
            self.workspace_name_for_panel(),
            self.pending.clone(),
        )
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
        let synthesized = self.synthesize_app_config_for_auth(config);
        let ws_name = self.workspace_name_for_panel();
        crate::tui::screens::editor::update::auth_flat_rows(
            self.auth_selected_kind,
            crate::tui::auth::AuthKind::WORKSPACE_PANEL_KINDS
                .iter()
                .copied(),
            &self.pending.roles,
            self.pending.allowed_roles.len(),
            &self.auth_expanded,
            &crate::tui::screens::editor::update::AuthFlatRowPredicates {
                role_override_present: &|kind, role| {
                    crate::tui::auth_config::role_override_present(*kind, role)
                },
                effective_mode_needs_credential: &|kind, role| {
                    crate::tui::auth_config::panel_mode_requires_credential(
                        &synthesized,
                        &ws_name,
                        role,
                        *kind,
                    )
                },
                effective_mode_supports_source_folder: &|kind, role| {
                    let mode = crate::tui::auth_config::resolve_panel_mode(
                        &synthesized,
                        *kind,
                        &ws_name,
                        role,
                    );
                    crate::tui::auth::auth_mode_supports_source_folder(*kind, mode)
                },
            },
        )
    }

    #[must_use]
    pub fn focused_auth_kind(
        &self,
        config: &jackin_config::AppConfig,
    ) -> Option<crate::tui::auth::AuthKind> {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.auth_flat_rows(config);
        match rows.get(n) {
            Some(AuthRow::AuthKindRow { kind }) => Some(*kind),
            _ => None,
        }
    }

    #[must_use]
    pub fn focused_auth_enter_plan(&self, config: &jackin_config::AppConfig) -> AuthEnterPlan {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.auth_flat_rows(config);
        match rows.get(n) {
            Some(AuthRow::AddSentinel { .. }) => AuthEnterPlan::AddRoleOverride,
            Some(AuthRow::RoleHeader { role, .. }) => AuthEnterPlan::ToggleRole(role.clone()),
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
            return if self.active_tab == EditorTab::Auth && self.auth_selected_kind.is_some() {
                EditorEscapeKeyPlan::FocusTabBarAndClearAuthKind
            } else {
                EditorEscapeKeyPlan::FocusTabBar
            };
        }

        if self.active_tab == EditorTab::Auth && self.auth_selected_kind.is_some() {
            EditorEscapeKeyPlan::ClearAuthKind
        } else {
            use crate::tui::screens::edit_save::{EditSaveDisposition, plan_leave_when_dirty};
            match plan_leave_when_dirty(self.is_dirty()) {
                EditSaveDisposition::ConfirmDiscard => EditorEscapeKeyPlan::OpenSaveDiscard,
                EditSaveDisposition::Noop | EditSaveDisposition::SaveNow => {
                    EditorEscapeKeyPlan::ReloadFromConfig
                }
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
        config: &jackin_config::AppConfig,
        expanded: bool,
    ) -> EditorRoleHeaderExpansionKeyPlan {
        match self.active_tab {
            EditorTab::Secrets => EditorRoleHeaderExpansionKeyPlan::Secrets(
                self.focused_secrets_role_expansion_plan(expanded),
            ),
            EditorTab::Auth => EditorRoleHeaderExpansionKeyPlan::Auth(
                self.focused_auth_role_expansion_plan(config, expanded),
            ),
            EditorTab::General | EditorTab::Mounts | EditorTab::Roles => {
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
        config: &jackin_config::AppConfig,
        key_code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) -> EditorImmediateActionKeyPlan {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key_code {
            KeyCode::Enter if self.active_tab == EditorTab::Auth => self
                .focused_auth_kind(config)
                .map_or(EditorImmediateActionKeyPlan::NotImmediateAction, |kind| {
                    EditorImmediateActionKeyPlan::EnterAuthKind(kind)
                }),
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
            KeyCode::Char('a' | 'A') if self.auth_selected_kind.is_some() => {
                EditorAuthActionKeyPlan::OpenRolePicker
            }
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
