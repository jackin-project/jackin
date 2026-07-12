#[allow(clippy::wildcard_imports, reason = "documented residual allow; prefer expect when site is lint-true")]
use super::super::*;

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
> crate::tui::debug::ConsoleEditorDebugFacts
    for EditorState<
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
where
    Modal: crate::tui::debug::ConsoleModalDebugKind,
{
    fn editor_stage_debug(&self) -> crate::tui::debug::ConsoleStageDebug {
        crate::tui::debug::ConsoleStageDebug::Editor {
            mode: format!("{:?}", self.mode),
            tab: format!("{:?}", self.active_tab),
            field: format!("{:?}", self.active_field),
            modal: self
                .modal
                .as_ref()
                .map(crate::tui::debug::ConsoleModalDebugKind::modal_debug_kind),
        }
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
            _env_value: PhantomData,
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

    pub fn apply_auth_kind_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorAuthKindPlan<crate::tui::auth::AuthKind>,
    ) {
        self.auth_selected_kind = plan.selected_kind;
        self.active_field = FieldFocus::Row(plan.active_row);
        self.tab_scroll_x = plan.tab_scroll_x;
        self.tab_scroll_y = plan.tab_scroll_y;
    }

    pub fn apply_tab_move_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorTabMovePlan,
    ) {
        self.active_tab = plan.active_tab;
        self.set_tab_bar_focused(plan.tab_bar_focused);
        self.active_field = FieldFocus::Row(plan.active_row);
        self.tab_scroll_x = plan.tab_scroll_x;
        self.tab_scroll_y = plan.tab_scroll_y;
        if plan.tab_bar_focused {
            self.set_workspace_mounts_scroll_focused(false);
            self.set_tab_content_scroll_focused(false);
        }
        if plan.clear_auth_kind {
            self.auth_selected_kind = None;
        }
        if plan.clear_secret_view_state {
            self.unmasked_rows.clear();
            self.secrets_expanded.clear();
        }
    }

    pub fn apply_tab_select_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorTabSelectPlan,
    ) {
        self.active_tab = plan.active_tab;
        self.set_tab_bar_focused(plan.tab_bar_focused);
        self.active_field = FieldFocus::Row(plan.active_row);
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
        if plan.clear_auth_kind {
            self.auth_selected_kind = None;
        }
        if plan.clear_secret_view_state {
            self.unmasked_rows.clear();
            self.secrets_expanded.clear();
        }
    }

    pub fn apply_field_selection_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorFieldSelectionPlan,
    ) {
        self.active_field = FieldFocus::Row(plan.active_row);
        self.tab_scroll_y = plan.tab_scroll_y;
    }

    pub fn apply_mount_row_select_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorMountRowSelectPlan,
    ) {
        self.active_field = FieldFocus::Row(plan.active_row);
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
    }

    pub fn select_row(&mut self, row: usize) {
        self.active_field = FieldFocus::Row(row);
    }

    pub fn select_auth_row(&mut self, row: usize) {
        self.select_row(row);
    }

    pub fn apply_tab_horizontal_scroll_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorHorizontalScrollPlan,
    ) {
        self.tab_scroll_x = plan.scroll_x;
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
        self.set_tab_content_scroll_focused(plan.tab_content_scroll_focused);
    }

    pub fn apply_workspace_mounts_horizontal_scroll_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorHorizontalScrollPlan,
    ) {
        self.workspace_mounts_scroll_x = plan.scroll_x;
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
        self.set_tab_content_scroll_focused(plan.tab_content_scroll_focused);
    }

    pub fn apply_scroll_focus_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorScrollFocusPlan,
    ) {
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
        self.set_tab_content_scroll_focused(plan.tab_content_scroll_focused);
    }

    #[must_use]
    pub const fn tab_bar_focused(&self) -> bool {
        self.focus_owner.is_tab_bar()
    }

    #[must_use]
    pub fn navigation_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
    ) -> EditorNavigationKeyPlan {
        use crossterm::event::KeyCode;

        match key_code {
            KeyCode::Left | KeyCode::BackTab if self.tab_bar_focused() => {
                EditorNavigationKeyPlan::MoveTab {
                    delta: -1,
                    focus_tab_bar: true,
                }
            }
            KeyCode::Right if self.tab_bar_focused() => EditorNavigationKeyPlan::MoveTab {
                delta: 1,
                focus_tab_bar: true,
            },
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j' | 'J') if self.tab_bar_focused() => {
                EditorNavigationKeyPlan::FocusContent
            }
            KeyCode::Tab => EditorNavigationKeyPlan::MoveTab {
                delta: 1,
                focus_tab_bar: true,
            },
            KeyCode::BackTab => EditorNavigationKeyPlan::FocusTabBar,
            _ => EditorNavigationKeyPlan::NotNavigation,
        }
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

    pub fn apply_tab_bar_focus_plan(&mut self, focused: bool) {
        self.set_tab_bar_focused(focused);
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

    pub fn set_hover_target(&mut self, target: Option<EditorHoverTarget>) {
        self.hover_target = target;
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

    pub fn new_create_with_workspace(name: String, workspace: WorkspaceConfig) -> Self
    where
        WorkspaceConfig: Clone,
        MountInfoCache: Default,
        SaveFlow: Default,
    {
        let mut editor = Self::new_edit(String::new(), workspace).into_create_mode();
        editor.pending_name = Some(name);
        editor
    }

    pub fn commit_workspace_name_input(&mut self, name: impl Into<String>) {
        self.pending_name = Some(name.into());
        self.clear_modal_chain();
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

    pub fn open_save_discard_cancel<SaveDiscardState>(&mut self, state: SaveDiscardState)
    where
        Modal: EditorSaveDiscardModal<SaveDiscardState>,
    {
        self.modal = Some(Modal::save_discard_cancel_modal(state));
    }

    pub fn open_error_popup<ErrorPopupState>(&mut self, state: ErrorPopupState)
    where
        Modal: EditorErrorPopupModal<ErrorPopupState>,
    {
        self.modal = Some(Modal::error_popup_modal(state));
    }

    pub fn pop_modal_chain(&mut self) {
        self.modal = self.modal_parents.pop();
    }

    pub fn clear_modal_chain(&mut self) {
        self.modal = None;
        self.modal_parents.clear();
    }

    pub fn dismiss_active_modal(&mut self) {
        self.modal = None;
    }

    #[must_use]
    pub fn has_modal_parent(&self) -> bool {
        !self.modal_parents.is_empty()
    }

    pub fn dismiss_status_popup(&mut self)
    where
        Modal: EditorStatusPopupModal,
    {
        if self
            .modal
            .as_ref()
            .is_some_and(EditorStatusPopupModal::is_status_popup)
        {
            self.modal = None;
        }
    }

    #[must_use]
    pub fn has_active_role_override_picker(&self) -> bool
    where
        Modal: EditorRoleOverridePickerModal,
    {
        self.modal
            .as_ref()
            .is_some_and(EditorRoleOverridePickerModal::is_role_override_picker)
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

    #[must_use]
    pub fn active_auth_form_focus(
        &self,
    ) -> Option<crate::tui::screens::settings::model::AuthFormFocus>
    where
        Modal: crate::tui::auth_config::ModalAuthFormFocusInspect<
                crate::tui::screens::settings::model::AuthFormFocus,
            >,
    {
        self.modal
            .as_ref()
            .and_then(crate::tui::auth_config::ModalAuthFormFocusInspect::active_auth_form_focus)
    }

    #[must_use]
    pub fn has_auth_form_parent(&self) -> bool
    where
        Modal: crate::tui::auth_config::ModalAuthFormParentInspect,
    {
        self.modal_parents
            .last()
            .is_some_and(crate::tui::auth_config::ModalAuthFormParentInspect::is_auth_form_parent)
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
