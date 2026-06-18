//! Editor screen state: draft workspace config being edited and per-tab/
//! per-field edit state for General, Mounts, Roles, Secrets, and Auth panels.
//!
//! Not responsible for: event handling (see `update`) or rendering (see
//! `view`).

use std::collections::{BTreeMap, BTreeSet};

use jackin_config::WorkspaceConfig;
use jackin_tui::components::FocusOwner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTab {
    General,
    Mounts,
    Roles,
    Secrets,
    Auth,
}

impl EditorTab {
    pub const ALL: [Self; 5] = [
        Self::General,
        Self::Mounts,
        Self::Roles,
        Self::Secrets,
        Self::Auth,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Mounts => "Mounts",
            Self::Roles => "Roles",
            Self::Secrets => "Environments",
            Self::Auth => "Auth",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorFocusTarget {
    WorkspaceMounts,
    TabContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorHoverTarget {
    Tab(usize),
    MountRow(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleHeaderExpansionPlan {
    Set { role: String, expanded: bool },
    HeaderNoop,
    NotHeader,
}

#[derive(Debug, Clone)]
pub enum EditorMode {
    Edit { name: String },
    Create,
}

#[derive(Debug)]
pub struct EditorState<
    WorkspaceConfig,
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
> {
    pub mode: EditorMode,
    pub active_tab: EditorTab,
    /// W3C ARIA Tabs: focus is either on the tab list or exactly one content block.
    pub focus_owner: FocusOwner<EditorFocusTarget>,
    pub hover_target: Option<EditorHoverTarget>,
    pub active_field: FieldFocus,
    pub original: WorkspaceConfig,
    pub pending: WorkspaceConfig,
    pub mount_info_cache: MountInfoCache,
    pub modal: Option<Modal>,
    pub modal_parents: Vec<Modal>,
    /// Create-mode only; Edit mode reads name from `EditorMode::Edit`.
    pub pending_name: Option<String>,
    /// Signals the outer input handler to save and/or pop to List.
    pub exit_after_save: Option<ExitIntent>,
    pub save_flow: SaveFlow,
    /// Secrets tab keys whose value is currently unmasked.
    pub unmasked_rows: BTreeSet<(SecretsScopeTag, String)>,
    pub secrets_expanded: BTreeSet<String>,
    pub auth_expanded: BTreeSet<String>,
    pub auth_selected_kind: Option<crate::tui::auth::AuthKind>,
    pub pending_picker_target: Option<(SecretsScopeTag, Option<String>)>,
    pub pending_picker_value: Option<EnvValue>,
    pub workspace_mounts_scroll_x: u16,
    pub tab_scroll_x: u16,
    pub tab_scroll_y: u16,
    pub tab_content_width: usize,
    pub tab_content_height: usize,
    pub generating_token_target: Option<AuthFormTarget>,
    pub pending_token_generate: Option<PendingTokenGenerate>,
    pub pending_role_load: Option<PendingRoleLoad>,
    pub pending_drift_check: Option<PendingDriftCheck>,
    pub pending_isolation_cleanup: Option<PendingIsolationCleanup>,
    pub pending_op_commit: Option<PendingOpCommit>,
    pub cached_footer_h: u16,
}

impl<
    WorkspaceConfig,
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
        WorkspaceConfig,
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
    pub fn new_edit(name: String, ws: WorkspaceConfig) -> Self
    where
        WorkspaceConfig: Clone,
        MountInfoCache: Default,
        SaveFlow: Default,
    {
        Self {
            mode: EditorMode::Edit { name },
            active_tab: EditorTab::General,
            focus_owner: FocusOwner::TabBar,
            hover_target: None,
            active_field: FieldFocus::Row(0),
            original: ws.clone(),
            pending: ws,
            mount_info_cache: MountInfoCache::default(),
            modal: None,
            modal_parents: Vec::new(),
            pending_name: None,
            exit_after_save: None,
            save_flow: SaveFlow::default(),
            unmasked_rows: BTreeSet::default(),
            secrets_expanded: BTreeSet::default(),
            auth_expanded: BTreeSet::default(),
            auth_selected_kind: None,
            pending_picker_target: None,
            pending_picker_value: None,
            workspace_mounts_scroll_x: 0,
            tab_scroll_x: 0,
            tab_scroll_y: 0,
            tab_content_width: 0,
            tab_content_height: 0,
            generating_token_target: None,
            pending_token_generate: None,
            pending_role_load: None,
            pending_drift_check: None,
            pending_isolation_cleanup: None,
            pending_op_commit: None,
            cached_footer_h: 1,
        }
    }

    #[must_use]
    pub const fn focus_owner(&self) -> FocusOwner<EditorFocusTarget> {
        self.focus_owner
    }

    pub fn set_focus_owner(&mut self, owner: FocusOwner<EditorFocusTarget>) {
        self.focus_owner = owner;
    }

    #[must_use]
    pub const fn tab_bar_focused(&self) -> bool {
        self.focus_owner.is_tab_bar()
    }

    #[must_use]
    pub const fn content_area(&self, term_size: ratatui::layout::Rect) -> ratatui::layout::Rect {
        crate::tui::layout::tabbed_content_area(term_size, self.cached_footer_h)
    }

    pub fn set_tab_bar_focused(&mut self, focused: bool) {
        self.focus_owner = if focused {
            FocusOwner::TabBar
        } else if matches!(self.active_tab, EditorTab::Mounts) {
            FocusOwner::Content(EditorFocusTarget::WorkspaceMounts)
        } else {
            FocusOwner::Content(EditorFocusTarget::TabContent)
        };
    }

    #[must_use]
    pub const fn workspace_mounts_scroll_focused(&self) -> bool {
        matches!(
            self.focus_owner,
            FocusOwner::Content(EditorFocusTarget::WorkspaceMounts)
        )
    }

    pub fn set_workspace_mounts_scroll_focused(&mut self, focused: bool) {
        if focused {
            self.focus_owner = FocusOwner::Content(EditorFocusTarget::WorkspaceMounts);
        } else if self.workspace_mounts_scroll_focused() {
            self.focus_owner = FocusOwner::TabBar;
        }
    }

    #[must_use]
    pub const fn tab_content_scroll_focused(&self) -> bool {
        matches!(
            self.focus_owner,
            FocusOwner::Content(EditorFocusTarget::TabContent)
        )
    }

    pub fn set_tab_content_scroll_focused(&mut self, focused: bool) {
        if focused {
            self.focus_owner = FocusOwner::Content(EditorFocusTarget::TabContent);
        } else if self.tab_content_scroll_focused() {
            self.focus_owner = FocusOwner::TabBar;
        }
    }

    #[must_use]
    pub const fn hovered_tab(&self) -> Option<usize> {
        match self.hover_target {
            Some(EditorHoverTarget::Tab(index)) => Some(index),
            _ => None,
        }
    }

    #[must_use]
    pub const fn hovered_mount_row(&self) -> Option<usize> {
        match self.hover_target {
            Some(EditorHoverTarget::MountRow(index)) => Some(index),
            _ => None,
        }
    }

    #[must_use]
    pub fn workspace_name_for_panel(&self) -> String {
        crate::tui::screens::editor::view::editor_name_value(
            &self.mode,
            self.pending_name.as_deref(),
            "(new workspace)",
        )
    }

    pub fn new_create() -> Self
    where
        WorkspaceConfig: Clone + Default,
        MountInfoCache: Default,
        SaveFlow: Default,
    {
        let empty = WorkspaceConfig::default();
        Self::new_edit(String::new(), empty).into_create_mode()
    }

    #[must_use]
    fn into_create_mode(mut self) -> Self {
        self.mode = EditorMode::Create;
        self
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

    #[must_use]
    pub fn auth_form_can_generate_token(&self) -> bool
    where
        Modal: crate::tui::auth_config::ModalAuthFormGenerate,
    {
        let editing_existing_workspace = matches!(self.mode, EditorMode::Edit { .. });
        self.modal
            .as_ref()
            .is_some_and(|modal| modal.auth_form_can_generate_token(editing_existing_workspace))
    }

    pub fn start_auth_token_generate<SourcePickerState>(
        &mut self,
        source_picker_state: SourcePickerState,
    ) -> bool
    where
        Modal: crate::tui::auth_config::ModalAuthFormGenerate
            + crate::tui::auth_config::ModalAuthTokenGenerateStart<AuthFormTarget, SourcePickerState>,
        AuthFormTarget: Clone,
    {
        if !self.auth_form_can_generate_token() {
            return false;
        }
        let Some(generate_target) = Modal::open_auth_generate_source_picker(
            &mut self.modal,
            &mut self.modal_parents,
            source_picker_state,
        ) else {
            return false;
        };
        self.generating_token_target = Some(generate_target);
        true
    }
}

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
        WorkspaceConfig,
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

    #[must_use]
    #[allow(unfulfilled_lint_expectations)]
    #[expect(
        single_use_lifetimes,
        reason = "impl Iterator over borrowed String keys cannot use anonymous lifetimes on stable Rust"
    )]
    pub fn eligible_role_override_selectors<'a>(
        &self,
        registered_roles: impl Iterator<Item = &'a String>,
    ) -> Vec<jackin_core::RoleSelector> {
        crate::workspace::eligible_role_keys_for_override(registered_roles, &self.pending)
            .into_iter()
            .filter_map(|name| jackin_core::RoleSelector::parse(&name).ok())
            .collect()
    }

    #[must_use]
    #[allow(unfulfilled_lint_expectations)]
    #[expect(
        single_use_lifetimes,
        reason = "impl Iterator over borrowed String keys cannot use anonymous lifetimes on stable Rust"
    )]
    pub fn auth_role_override_selectors<'a>(
        &self,
        registered_roles: impl Iterator<Item = &'a String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldFocus {
    Row(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecretsScopeTag {
    Workspace,
    Role(String),
}

/// Flat row model for the Secrets tab; cursor is a single index.
#[derive(Debug, Clone)]
pub enum SecretsRow {
    WorkspaceKeyRow(String),
    WorkspaceAddSentinel,
    RoleHeader {
        role: String,
        expanded: bool,
    },
    RoleKeyRow {
        role: String,
        key: String,
    },
    RoleAddSentinel(String),
    /// Non-focusable; cursor Up/Down skips over it.
    SectionSpacer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretsEnterPlan {
    EditValue { scope: SecretsScopeTag, key: String },
    OpenScopePicker,
    ExpandRole(String),
    AddRoleKey { scope: SecretsScopeTag },
    Noop,
}

/// Row-shape model for the Auth tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthRow<K> {
    /// Root picker row: choose which auth kind to manage.
    AuthKindRow { kind: K },
    /// Selected auth kind's workspace-level mode row.
    WorkspaceMode { kind: K },
    /// Selected auth kind's workspace credential source row.
    WorkspaceSource { kind: K },
    /// Selected auth kind's workspace sync source-folder row.
    WorkspaceSourceFolder { kind: K },
    /// Collapsible role override block.
    RoleHeader { role: String, expanded: bool },
    /// Mode row inside an expanded `RoleHeader`.
    RoleMode { role: String, kind: K },
    /// Credential source row inside an expanded `RoleHeader`.
    RoleSource { role: String, kind: K },
    /// Sync source-folder row inside an expanded `RoleHeader`.
    RoleSourceFolder { role: String, kind: K },
    /// `+ Override for a role` sentinel.
    AddSentinel { eligible: usize },
    /// Visual spacer.
    Spacer,
}

#[derive(Debug, Clone)]
pub struct PendingSaveCommit<M> {
    pub effective_removals: Vec<String>,
    pub final_mounts: Option<Vec<M>>,
    /// True when the operator has already confirmed isolated-state cleanup
    /// for source drift in this save cycle.
    pub delete_isolated_acknowledged: bool,
    /// True after the acknowledged cleanup worker has completed; the final
    /// write pass can then skip drift re-check and cleanup.
    pub isolated_cleanup_complete: bool,
}

#[derive(Debug, Clone, Default)]
pub enum EditorSaveFlow<P> {
    #[default]
    Idle,
    Confirming {
        exit_on_success: bool,
    },
    PendingCommit {
        plan: P,
        exit_on_success: bool,
    },
    Error {
        message: String,
    },
}

impl<P> EditorSaveFlow<P> {
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    #[must_use]
    pub const fn error_message(&self) -> Option<&str> {
        if let Self::Error { message } = self {
            Some(message.as_str())
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConfirmTarget<R, P> {
    DeleteEnvVar {
        scope: SecretsScopeTag,
        key: String,
    },
    TrustRoleSource {
        key: String,
        source: R,
    },
    DeleteIsolatedAndSave {
        plan: P,
        exit_on_success: bool,
        affected_containers: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInputTarget {
    Name,
    Workdir,
    MountDst,
    Role,
    EnvKey { scope: SecretsScopeTag },
    EnvValue { scope: SecretsScopeTag, key: String },
    AuthCredential,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileBrowserTarget {
    CreateFirstMountSrc,
    EditAddMountSrc,
    AuthFormSourceFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitIntent {
    Save,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateStep {
    PickFirstMountSrc,
    PickFirstMountDst,
    PickWorkdir,
    NameWorkspace,
}

#[cfg(test)]
mod tests {
    use jackin_config::{
        MountConfig, MountIsolation, RoleSource, WorkspaceConfig, WorkspaceRoleOverride,
    };

    use super::{AuthRow, EditorState, EditorTab, FieldFocus, RoleHeaderExpansionPlan, SecretsRow};

    type TestEditor =
        EditorState<WorkspaceConfig, (), (), (), jackin_config::EnvValue, (), (), (), (), (), ()>;

    #[test]
    fn editor_dirty_tracks_pending_config_and_rename() {
        let workspace = WorkspaceConfig {
            workdir: "/work".into(),
            ..Default::default()
        };
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        assert!(!editor.is_dirty());
        editor.pending_name = Some("beta".into());
        assert!(editor.is_dirty());
    }

    #[test]
    fn editor_workspace_name_for_panel_uses_create_fallback_or_pending_name() {
        let mut editor = TestEditor::new_create();

        assert_eq!(editor.workspace_name_for_panel(), "(new workspace)");

        editor.pending_name = Some("draft".into());
        assert_eq!(editor.workspace_name_for_panel(), "draft");
    }

    #[test]
    fn editor_synthesizes_pending_workspace_for_auth_rows() {
        let mut editor = TestEditor::new_create();
        editor.pending_name = Some("draft".into());
        editor.pending.env.insert(
            jackin_core::env_model::ZAI_API_KEY_ENV_NAME.into(),
            jackin_config::EnvValue::Plain("zai".into()),
        );
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Zai);

        let synthesized =
            editor.synthesize_app_config_for_auth(&jackin_config::AppConfig::default());
        let rows = editor.auth_flat_rows(&jackin_config::AppConfig::default());

        assert!(synthesized.workspaces.contains_key("draft"));
        assert!(rows.iter().any(|row| matches!(
            row,
            AuthRow::WorkspaceMode {
                kind: crate::tui::auth::AuthKind::Zai
            }
        )));
    }

    #[test]
    fn editor_focused_auth_form_prefills_workspace_layer() {
        let workspace = WorkspaceConfig {
            claude: Some(jackin_config::AgentAuthConfig {
                auth_forward: jackin_config::AuthForwardMode::Sync,
                sync_source_dir: Some(std::path::PathBuf::from("/host/claude")),
            }),
            ..WorkspaceConfig::default()
        };
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);

        let (target, form) = editor
            .focused_auth_form(&jackin_config::AppConfig::default())
            .expect("workspace mode row should open auth form");

        assert!(matches!(
            target,
            crate::tui::screens::settings::model::AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Claude
            }
        ));
        assert_eq!(form.mode, Some(crate::tui::auth::AuthMode::Sync));
        assert_eq!(
            form.source_folder,
            Some(std::path::PathBuf::from("/host/claude"))
        );
        assert!(form.shows_source_folder());
    }

    #[test]
    fn editor_focused_auth_form_returns_none_for_non_form_rows() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);
        editor.active_field = FieldFocus::Row(usize::MAX);

        assert!(
            editor
                .focused_auth_form(&jackin_config::AppConfig::default())
                .is_none()
        );
    }

    #[test]
    fn editor_persist_auth_form_writes_workspace_layer() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        let mut form =
            crate::tui::components::auth_panel::AuthForm::new(crate::tui::auth::AuthKind::Zai);
        form.set_mode(crate::tui::auth::AuthMode::ApiKey);
        form.set_literal("zai-key".into());

        editor.persist_auth_form(
            &crate::tui::screens::settings::model::AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Zai,
            },
            &form,
        );

        assert_eq!(
            editor
                .pending
                .env
                .get(jackin_core::env_model::ZAI_API_KEY_ENV_NAME),
            Some(&jackin_config::EnvValue::Plain("zai-key".into()))
        );
    }

    #[test]
    fn editor_clear_auth_form_layer_clears_role_source_folder() {
        let mut workspace = WorkspaceConfig::default();
        workspace.roles.entry("dev".into()).or_default().claude =
            Some(jackin_config::AgentAuthConfig {
                auth_forward: jackin_config::AuthForwardMode::Sync,
                sync_source_dir: Some(std::path::PathBuf::from("/role/claude")),
            });
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        editor.clear_auth_form_layer(
            &crate::tui::screens::settings::model::AuthFormTarget::WorkspaceRole {
                role: "dev".into(),
                kind: crate::tui::auth::AuthKind::Claude,
            },
        );

        assert_eq!(editor.pending.roles["dev"].claude, None);
    }

    #[test]
    fn editor_toggle_auth_role_expanded_flips_role_section() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

        editor.toggle_auth_role_expanded("dev".into());
        assert!(editor.auth_expanded.contains("dev"));

        editor.toggle_auth_role_expanded("dev".into());
        assert!(!editor.auth_expanded.contains("dev"));
    }

    #[test]
    fn editor_focused_auth_role_expansion_plan_reads_current_row() {
        let workspace = WorkspaceConfig {
            roles: std::collections::BTreeMap::from([(
                "dev".into(),
                WorkspaceRoleOverride {
                    github: Some(jackin_config::GithubAuthConfig {
                        auth_forward: jackin_config::GithubAuthMode::Token,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Github);
        let config = jackin_config::AppConfig::default();
        editor.active_field = FieldFocus::Row(
            editor
                .auth_flat_rows(&config)
                .iter()
                .position(|row| matches!(row, AuthRow::RoleHeader { role, .. } if role == "dev"))
                .expect("role header row"),
        );

        assert_eq!(
            editor.focused_auth_role_expansion_plan(&config, true),
            RoleHeaderExpansionPlan::Set {
                role: "dev".into(),
                expanded: true
            }
        );

        editor.auth_expanded.insert("dev".into());
        assert_eq!(
            editor.focused_auth_role_expansion_plan(&config, true),
            RoleHeaderExpansionPlan::HeaderNoop
        );
        assert_eq!(
            editor.focused_auth_role_expansion_plan(&config, false),
            RoleHeaderExpansionPlan::Set {
                role: "dev".into(),
                expanded: false
            }
        );
    }

    #[test]
    fn editor_focused_auth_kind_reads_current_row() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        let config = jackin_config::AppConfig::default();

        assert_eq!(
            editor.focused_auth_kind(&config),
            Some(crate::tui::auth::AuthKind::Claude)
        );

        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);
        assert_eq!(editor.focused_auth_kind(&config), None);
    }

    #[test]
    fn editor_clear_auth_row_at_cursor_clears_workspace_auth_layer() {
        let workspace = WorkspaceConfig {
            env: std::collections::BTreeMap::from([(
                jackin_core::env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
                jackin_config::EnvValue::Plain("zai".into()),
            )]),
            ..WorkspaceConfig::default()
        };
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Zai);

        editor.clear_auth_row_at_cursor(&jackin_config::AppConfig::default());

        assert!(
            !editor
                .pending
                .env
                .contains_key(jackin_core::env_model::ZAI_API_KEY_ENV_NAME)
        );
    }

    #[test]
    fn editor_clear_auth_row_at_cursor_clears_role_auth_layer() {
        let mut workspace = WorkspaceConfig::default();
        workspace.roles.entry("dev".into()).or_default().env.insert(
            jackin_core::env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
            jackin_config::EnvValue::Plain("zai".into()),
        );
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Zai);

        let rows = editor.auth_flat_rows(&jackin_config::AppConfig::default());
        editor.active_field = FieldFocus::Row(
            rows.iter()
                .position(|row| matches!(row, AuthRow::RoleHeader { role, .. } if role == "dev"))
                .expect("role header should be present"),
        );
        editor.clear_auth_row_at_cursor(&jackin_config::AppConfig::default());

        assert!(
            !editor.pending.roles["dev"]
                .env
                .contains_key(jackin_core::env_model::ZAI_API_KEY_ENV_NAME)
        );
    }

    #[test]
    fn editor_secrets_flat_rows_reads_pending_workspace_env() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor
            .pending
            .env
            .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));

        assert!(editor.secrets_flat_rows().iter().any(|row| matches!(
            row,
            SecretsRow::WorkspaceKeyRow(key) if key == "TOKEN"
        )));
    }

    #[test]
    fn editor_selection_bounds_reads_state_and_config_counts() {
        let workspace = WorkspaceConfig {
            mounts: vec![
                MountConfig {
                    src: "/src-a".into(),
                    dst: "/dst-a".into(),
                    readonly: false,
                    isolation: MountIsolation::Shared,
                },
                MountConfig {
                    src: "/src-b".into(),
                    dst: "/dst-b".into(),
                    readonly: true,
                    isolation: MountIsolation::Shared,
                },
            ],
            ..Default::default()
        };
        let mut config = jackin_config::AppConfig::default();
        config.roles.insert("alpha".into(), RoleSource::default());
        config.roles.insert("beta".into(), RoleSource::default());
        config.roles.insert("gamma".into(), RoleSource::default());
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        editor.active_tab = EditorTab::Mounts;
        assert_eq!(editor.selection_bounds(&config), (2, Vec::new()));

        editor.active_tab = EditorTab::Roles;
        assert_eq!(editor.selection_bounds(&config), (3, Vec::new()));
    }

    #[test]
    fn editor_secret_value_reads_workspace_and_role_env() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor
            .pending
            .env
            .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));
        editor
            .pending
            .roles
            .entry("dev".into())
            .or_default()
            .env
            .insert(
                "ROLE_TOKEN".into(),
                jackin_config::EnvValue::OpRef(jackin_core::OpRef {
                    op: "op://vault/item/field".into(),
                    path: "Vault/Item/Field".into(),
                    account: None,
                }),
            );

        assert_eq!(
            editor.secret_value(&super::SecretsScopeTag::Workspace, "TOKEN"),
            Some(&jackin_config::EnvValue::Plain("one".into()))
        );
        assert!(
            editor
                .secret_value(&super::SecretsScopeTag::Role("dev".into()), "ROLE_TOKEN")
                .is_some_and(|value| matches!(value, jackin_config::EnvValue::OpRef(_)))
        );
        assert!(
            editor
                .secret_value(
                    &super::SecretsScopeTag::Role("missing".into()),
                    "ROLE_TOKEN"
                )
                .is_none()
        );
    }

    #[test]
    fn editor_secret_text_editability_rejects_op_refs() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor
            .pending
            .env
            .insert("PLAIN".into(), jackin_config::EnvValue::Plain("one".into()));
        editor.pending.env.insert(
            "OP_REF".into(),
            jackin_config::EnvValue::OpRef(jackin_core::OpRef {
                op: "op://vault/item/field".into(),
                path: "Vault/Item/Field".into(),
                account: None,
            }),
        );

        assert!(editor.secret_is_text_editable(&super::SecretsScopeTag::Workspace, "PLAIN"));
        assert!(!editor.secret_is_text_editable(&super::SecretsScopeTag::Workspace, "OP_REF"));
    }

    #[test]
    fn editor_focused_unmask_key_skips_op_refs() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.pending.env.insert(
            "A_TOKEN".into(),
            jackin_config::EnvValue::Plain("one".into()),
        );
        editor.pending.env.insert(
            "Z_OP_REF".into(),
            jackin_config::EnvValue::OpRef(jackin_core::OpRef {
                op: "op://vault/item/field".into(),
                path: "Vault/Item/Field".into(),
                account: None,
            }),
        );

        editor.active_field = FieldFocus::Row(0);
        assert_eq!(
            editor.focused_unmask_key(),
            Some((super::SecretsScopeTag::Workspace, "A_TOKEN".into()))
        );

        editor.active_field = FieldFocus::Row(1);
        assert_eq!(editor.focused_unmask_key(), None);
    }

    #[test]
    fn editor_focused_secret_enter_plan_reads_current_row() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor
            .pending
            .env
            .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));

        assert_eq!(
            editor.focused_secret_enter_plan(),
            super::SecretsEnterPlan::EditValue {
                scope: super::SecretsScopeTag::Workspace,
                key: "TOKEN".into()
            }
        );

        editor.active_field = FieldFocus::Row(1);
        assert_eq!(
            editor.focused_secret_enter_plan(),
            super::SecretsEnterPlan::Noop
        );

        editor.active_field = FieldFocus::Row(2);
        assert_eq!(
            editor.focused_secret_enter_plan(),
            super::SecretsEnterPlan::OpenScopePicker
        );
    }

    #[test]
    fn editor_focused_secrets_role_expansion_plan_reads_current_row() {
        let workspace = WorkspaceConfig {
            roles: std::collections::BTreeMap::from([(
                "dev".into(),
                WorkspaceRoleOverride::default(),
            )]),
            ..Default::default()
        };
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.active_field = FieldFocus::Row(
            editor
                .secrets_flat_rows()
                .iter()
                .position(|row| matches!(row, SecretsRow::RoleHeader { role, .. } if role == "dev"))
                .expect("role header row"),
        );

        assert_eq!(
            editor.focused_secrets_role_expansion_plan(true),
            RoleHeaderExpansionPlan::Set {
                role: "dev".into(),
                expanded: true
            }
        );

        editor.secrets_expanded.insert("dev".into());
        assert_eq!(
            editor.focused_secrets_role_expansion_plan(true),
            RoleHeaderExpansionPlan::HeaderNoop
        );
        assert_eq!(
            editor.focused_secrets_role_expansion_plan(false),
            RoleHeaderExpansionPlan::Set {
                role: "dev".into(),
                expanded: false
            }
        );
    }

    #[test]
    fn editor_focused_secret_targets_read_current_row() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor
            .pending
            .env
            .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));

        assert_eq!(
            editor.focused_secret_delete_target(),
            Some((super::SecretsScopeTag::Workspace, "TOKEN".into()))
        );
        assert_eq!(
            editor.focused_secret_add_target(),
            Some(super::SecretsScopeTag::Workspace)
        );

        editor.active_field = FieldFocus::Row(1);
        assert_eq!(editor.focused_secret_delete_target(), None);
        assert_eq!(editor.focused_secret_add_target(), None);
    }

    #[test]
    fn editor_change_count_tracks_env_and_role_auth() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        assert_eq!(editor.change_count(), 0);

        editor
            .pending
            .env
            .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));
        editor.pending.roles.entry("dev".into()).or_default().github =
            Some(jackin_config::GithubAuthConfig {
                auth_forward: jackin_config::GithubAuthMode::Token,
                ..Default::default()
            });

        assert_eq!(editor.change_count(), 4);
    }

    #[test]
    fn editor_cycle_isolation_for_selected_mount_updates_pending_mount() {
        let mut workspace = WorkspaceConfig::default();
        workspace.mounts.push(MountConfig {
            src: "/host".into(),
            dst: "/work".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        });
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        editor.cycle_isolation_for_selected_mount();

        assert_eq!(editor.pending.mounts[0].isolation, MountIsolation::Worktree);
    }

    #[test]
    fn editor_remove_selected_mount_deletes_pending_mount() {
        let mut workspace = WorkspaceConfig::default();
        workspace.mounts.push(MountConfig {
            src: "/host".into(),
            dst: "/work".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        });
        workspace.mounts.push(MountConfig {
            src: "/host2".into(),
            dst: "/work2".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        });
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.active_field = FieldFocus::Row(1);

        editor.remove_selected_mount();

        assert_eq!(editor.pending.mounts.len(), 1);
        assert_eq!(editor.pending.mounts[0].src, "/host");
    }

    #[test]
    fn editor_eligible_role_override_selectors_use_workspace_allowed_roles() {
        let mut workspace = WorkspaceConfig {
            allowed_roles: vec!["beta".into()],
            ..Default::default()
        };
        workspace.roles.entry("alpha".into()).or_default();
        let editor = TestEditor::new_edit("alpha".into(), workspace);
        let registered = ["alpha".to_owned(), "beta".to_owned(), "bad role".to_owned()];

        let eligible = editor.eligible_role_override_selectors(registered.iter());

        assert_eq!(eligible.len(), 1);
        assert_eq!(eligible[0].name.as_str(), "beta");
    }

    #[test]
    fn editor_auth_role_override_selectors_filter_existing_overrides() {
        let mut workspace = WorkspaceConfig {
            allowed_roles: vec!["alpha".into(), "beta".into()],
            ..Default::default()
        };
        workspace.roles.entry("alpha".into()).or_default().github =
            Some(jackin_config::GithubAuthConfig {
                auth_forward: jackin_config::GithubAuthMode::Token,
                ..Default::default()
            });
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Github);
        let registered = ["alpha".to_owned(), "beta".to_owned(), "bad role".to_owned()];

        let eligible = editor
            .auth_role_override_selectors(registered.iter())
            .expect("selected kind should produce candidates");

        assert_eq!(eligible.len(), 1);
        assert_eq!(eligible[0].name.as_str(), "beta");
    }

    #[test]
    fn editor_auth_role_override_selectors_require_selected_kind() {
        let editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        let registered = ["alpha".to_owned()];

        assert!(
            editor
                .auth_role_override_selectors(registered.iter())
                .is_none()
        );
    }

    #[test]
    fn editor_toggle_allowed_role_at_cursor_updates_pending_allow_list_and_default() {
        let workspace = WorkspaceConfig {
            default_role: Some("alpha".into()),
            ..Default::default()
        };
        let role_names = vec!["alpha".to_owned(), "beta".to_owned()];
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        editor.toggle_allowed_role_at_cursor(&role_names);

        assert_eq!(editor.pending.allowed_roles, vec!["beta".to_owned()]);
        assert_eq!(editor.pending.default_role, None);
    }

    #[test]
    fn editor_toggle_default_role_at_cursor_only_sets_allowed_role() {
        let role_names = vec!["alpha".to_owned(), "beta".to_owned()];
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.active_field = FieldFocus::Row(1);

        editor.toggle_default_role_at_cursor(&role_names);
        assert_eq!(editor.pending.default_role.as_deref(), Some("beta"));

        editor.pending.allowed_roles = vec!["alpha".into()];
        editor.pending.default_role = None;
        editor.toggle_default_role_at_cursor(&role_names);
        assert_eq!(editor.pending.default_role, None);
    }
}
