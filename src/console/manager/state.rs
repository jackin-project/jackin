//! Manager state machine. See docs/superpowers/specs/2026-04-23-workspace-manager-tui-design.md § 3.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;

use crate::config::AppConfig;
use crate::console::op_cache::OpCache;
use crate::workspace::WorkspaceConfig;

use crate::console::widgets::{
    agent_picker::AgentPickerState, confirm::ConfirmState, confirm_save::ConfirmSaveState,
    error_popup::ErrorPopupState, file_browser::FileBrowserState, github_picker::GithubPickerState,
    mount_dst_choice::MountDstChoiceState, op_picker::OpPickerState,
    scope_picker::ScopePickerState, source_picker::SourcePickerState, text_input::TextInputState,
    workdir_pick::WorkdirPickState,
};

/// Logical row in the manager list. Prefer over the raw `selected:
/// usize` when reasoning about what the operator is pointing at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerListRow {
    CurrentDirectory,
    SavedWorkspace(usize),
    NewWorkspace,
}

impl ManagerListRow {
    #[must_use]
    pub const fn to_screen_index(self, saved_count: usize) -> usize {
        match self {
            Self::CurrentDirectory => 0,
            Self::SavedWorkspace(i) => i + 1,
            Self::NewWorkspace => saved_count + 1,
        }
    }
}

#[derive(Debug)]
pub struct ManagerState<'a> {
    pub stage: ManagerStage<'a>,
    pub workspaces: Vec<WorkspaceSummary>,
    pub selected: usize,
    pub toast: Option<Toast>,
    /// Modal slot at the list level (e.g. `Modal::GithubPicker`); the
    /// Editor / `CreatePrelude` stages own their own modal slots.
    pub list_modal: Option<Modal<'a>>,
    pub list_split_pct: u16,
    pub drag_state: Option<DragState>,
    /// Process-lifetime cache of `op` structural metadata, threaded
    /// into the picker on open. Carries no credentials — see
    /// `op_cache.rs`.
    pub op_cache: Rc<RefCell<OpCache>>,
    /// Mirrored from `ConsoleState::op_available` (probed once at
    /// startup) so the Secrets-tab editor can disable the
    /// source-picker's 1Password choice without re-probing.
    pub op_available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragState {
    pub anchor_pct: u16,
    pub anchor_x: u16,
}

pub const MIN_SPLIT_PCT: u16 = 20;
pub const MAX_SPLIT_PCT: u16 = 80;
pub const DEFAULT_SPLIT_PCT: u16 = 30;

#[must_use]
pub const fn clamp_split(pct: u16) -> u16 {
    if pct < MIN_SPLIT_PCT {
        MIN_SPLIT_PCT
    } else if pct > MAX_SPLIT_PCT {
        MAX_SPLIT_PCT
    } else {
        pct
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ManagerStage<'a> {
    List,
    Editor(EditorState<'a>),
    CreatePrelude(CreatePreludeState<'a>),
    ConfirmDelete { name: String, state: ConfirmState },
}

#[derive(Debug, Clone)]
pub struct WorkspaceSummary {
    pub name: String,
    pub workdir: String,
    pub mount_count: usize,
    pub readonly_mount_count: usize,
    pub allowed_agent_count: usize,
    pub default_agent: Option<String>,
    pub last_agent: Option<String>,
}

#[derive(Debug)]
pub struct EditorState<'a> {
    pub mode: EditorMode,
    pub active_tab: EditorTab,
    pub active_field: FieldFocus,
    pub original: WorkspaceConfig,
    pub pending: WorkspaceConfig,
    pub modal: Option<Modal<'a>>,
    /// Create-mode only; Edit mode reads name from `EditorMode::Edit`.
    pub pending_name: Option<String>,
    /// Signals the outer `handle_key` to save and/or pop to List.
    pub exit_after_save: Option<ExitIntent>,
    pub save_flow: EditorSaveFlow,
    /// Secrets tab: keys whose value is currently unmasked. Cleared on
    /// tab leave so re-entry starts all-masked. Op:// rows ignore this
    /// — they render as a breadcrumb, not a masked value.
    pub unmasked_rows: BTreeSet<(SecretsScopeTag, String)>,
    pub secrets_expanded: BTreeSet<String>,
    /// Scratch for the two-step add flow: set on `EnvKey` commit,
    /// cleared on `EnvValue` commit/cancel.
    pub pending_env_key: Option<(SecretsScopeTag, String)>,
    /// Stashed by `P` on a Secrets row so `OpPicker` knows where to
    /// write its `op://` path. `Some((scope, Some(key)))` replaces a
    /// row's value; `Some((scope, None))` opens the `EnvKey` modal
    /// next with the value pre-stashed in `pending_picker_value`.
    pub pending_picker_target: Option<(SecretsScopeTag, Option<String>)>,
    /// In the sentinel-add flow, holds the picker-supplied path until
    /// the operator names the key and the `EnvKey` modal commits both
    /// fields at once.
    pub pending_picker_value: Option<String>,
}

/// Save cycle state machine.
///
/// `Idle` → (open `ConfirmSave`) `Confirming` → (stash plan)
/// `PendingCommit` → (outer loop writes to disk) `Idle` or `Error`.
/// `exit_on_success` is true when save came from `SaveDiscardCancel`
/// — outer loop pops to list on success. Pre-commit validation
/// errors land in `Error` and render as an inline banner instead of
/// a modal.
#[derive(Debug, Clone, Default)]
pub enum EditorSaveFlow {
    #[default]
    Idle,
    Confirming {
        exit_on_success: bool,
    },
    PendingCommit {
        plan: PendingSaveCommit,
        exit_on_success: bool,
    },
    Error {
        message: String,
    },
}

impl EditorSaveFlow {
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
pub struct PendingSaveCommit {
    pub effective_removals: Vec<String>,
    pub final_mounts: Option<Vec<crate::workspace::MountConfig>>,
}

#[derive(Debug, Clone)]
pub enum EditorMode {
    Edit { name: String },
    Create,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTab {
    General,
    Mounts,
    Agents,
    Secrets,
}

#[derive(Debug, Clone, Copy)]
pub enum FieldFocus {
    Row(usize),
}

// `TextInputState` is ~600B while other variants are ~330B. Boxing the state
// field would cascade through 19 construction/match sites (including wizard
// step transitions that move state in and out of `Modal`). The ergonomic cost
// is worse than the small stack-size win here, so we accept the variance.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Modal<'a> {
    TextInput {
        target: TextInputTarget,
        state: TextInputState<'a>,
    },
    FileBrowser {
        target: FileBrowserTarget,
        state: FileBrowserState,
    },
    MountDstChoice {
        target: FileBrowserTarget,
        state: MountDstChoiceState,
    },
    WorkdirPick {
        state: WorkdirPickState,
    },
    Confirm {
        target: ConfirmTarget,
        state: ConfirmState,
    },
    SaveDiscardCancel {
        state: crate::console::widgets::save_discard::SaveDiscardState,
    },
    /// Workspace list, when ≥2 GitHub mounts and operator pressed `o`.
    GithubPicker {
        state: GithubPickerState,
    },
    ConfirmSave {
        state: ConfirmSaveState,
    },
    ErrorPopup {
        state: ErrorPopupState,
    },
    /// Boxed because the picker's `Vec`s + runner + channel are
    /// substantially larger than other variants.
    OpPicker {
        state: Box<OpPickerState>,
    },
    /// Manager-list disambiguation picker (`ManagerState.list_modal`
    /// slot, same as `GithubPicker`).
    AgentPicker {
        state: AgentPickerState,
    },
    /// Editor-tab override picker (`EditorState.modal` slot, not the
    /// launch-disambiguation slot on `ManagerState`) so the editor's
    /// commit handler can create the override entry and auto-expand.
    AgentOverridePicker {
        state: AgentPickerState,
    },
    SourcePicker {
        state: SourcePickerState,
    },
    ScopePicker {
        state: ScopePickerState,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInputTarget {
    Name,
    Workdir,
    MountDst,
    EnvKey { scope: SecretsScopeTag },
    EnvValue { scope: SecretsScopeTag, key: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileBrowserTarget {
    CreateFirstMountSrc,
    EditAddMountSrc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmTarget {
    DeleteWorkspace,
    DeleteEnvVar { scope: SecretsScopeTag, key: String },
}

/// Separate from [`crate::config::editor::EnvScope`].
///
/// That type needs the workspace name, which Create mode hasn't
/// captured until `pending_name` lands at save time. The full
/// `EnvScope` is derived in `commit_editor_save`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecretsScopeTag {
    Workspace,
    Agent(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitIntent {
    Save,
    Discard,
}

#[derive(Debug)]
pub struct CreatePreludeState<'a> {
    pub step: CreateStep,
    pub pending_mount_src: Option<PathBuf>,
    pub pending_mount_dst: Option<String>,
    pub pending_readonly: bool,
    pub pending_workdir: Option<String>,
    pub pending_name: Option<String>,
    pub modal: Option<Modal<'a>>,
    /// Captured so Esc on `MountDstChoice` re-opens `FileBrowser` at
    /// the same directory instead of `$HOME`.
    pub last_browser_cwd: Option<PathBuf>,
    /// Picks Esc-on-`WorkdirPick` rewind target: `TextInputDst` when
    /// the Edit-destination branch was used, else `MountDstChoice`.
    pub used_edit_dst: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateStep {
    PickFirstMountSrc,
    PickFirstMountDst,
    PickWorkdir,
    NameWorkspace,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub shown_at: std::time::Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
}

// ── Impls ──────────────────────────────────────────────────────────

impl WorkspaceSummary {
    pub fn from_config(name: &str, ws: &WorkspaceConfig) -> Self {
        Self {
            name: name.to_string(),
            workdir: ws.workdir.clone(),
            mount_count: ws.mounts.len(),
            readonly_mount_count: ws.mounts.iter().filter(|m| m.readonly).count(),
            allowed_agent_count: ws.allowed_agents.len(),
            default_agent: ws.default_agent.clone(),
            last_agent: ws.last_agent.clone(),
        }
    }
}

impl ManagerState<'_> {
    /// Allocates a fresh empty cache and assumes `op` unavailable —
    /// production reset paths use the `_with_cache_and_op` variant to
    /// preserve the `ConsoleState`-owned cache.
    pub fn from_config(config: &AppConfig, cwd: &std::path::Path) -> Self {
        Self::from_config_with_cache(config, cwd, Rc::new(RefCell::new(OpCache::default())))
    }

    pub fn from_config_with_cache(
        config: &AppConfig,
        cwd: &std::path::Path,
        op_cache: Rc<RefCell<OpCache>>,
    ) -> Self {
        Self::from_config_with_cache_and_op(config, cwd, op_cache, false)
    }

    pub fn from_config_with_cache_and_op(
        config: &AppConfig,
        cwd: &std::path::Path,
        op_cache: Rc<RefCell<OpCache>>,
        op_available: bool,
    ) -> Self {
        let workspaces: Vec<WorkspaceSummary> = config
            .workspaces
            .iter()
            .map(|(name, ws)| WorkspaceSummary::from_config(name, ws))
            .collect();

        let saved_count = workspaces.len();
        let matching_saved = crate::app::context::find_saved_workspace_for_cwd(config, cwd)
            .and_then(|(name, _)| workspaces.iter().position(|w| w.name == name));
        let selected_row = matching_saved.map_or(
            ManagerListRow::CurrentDirectory,
            ManagerListRow::SavedWorkspace,
        );
        let selected = selected_row.to_screen_index(saved_count);

        Self {
            stage: ManagerStage::List,
            workspaces,
            selected,
            toast: None,
            list_modal: None,
            list_split_pct: DEFAULT_SPLIT_PCT,
            drag_state: None,
            op_cache,
            op_available,
        }
    }

    /// Total number of rows in the list (current-dir + saved + sentinel).
    #[must_use]
    pub const fn row_count(&self) -> usize {
        self.workspaces.len() + 2
    }

    /// Index of the "+ New workspace" sentinel row.
    #[must_use]
    pub const fn new_workspace_row_index(&self) -> usize {
        self.workspaces.len() + 1
    }

    /// Decode a raw screen-row `usize` into a [`ManagerListRow`]. Returns
    /// `None` when `idx` is out of range.
    #[must_use]
    pub const fn row_at(&self, idx: usize) -> Option<ManagerListRow> {
        let saved_count = self.workspaces.len();
        if idx == 0 {
            Some(ManagerListRow::CurrentDirectory)
        } else if idx == saved_count + 1 {
            Some(ManagerListRow::NewWorkspace)
        } else if idx <= saved_count {
            Some(ManagerListRow::SavedWorkspace(idx - 1))
        } else {
            None
        }
    }

    /// What the operator currently has highlighted.
    #[must_use]
    pub fn selected_row(&self) -> ManagerListRow {
        self.row_at(self.selected)
            .unwrap_or(ManagerListRow::CurrentDirectory)
    }

    /// Convenience: `true` when the selection is on the synthetic
    /// "Current directory" row.
    #[must_use]
    pub fn is_current_dir_selected(&self) -> bool {
        matches!(self.selected_row(), ManagerListRow::CurrentDirectory)
    }

    /// Convenience: `true` when the selection is on the "+ New workspace"
    /// sentinel.
    #[must_use]
    pub fn is_new_workspace_selected(&self) -> bool {
        matches!(self.selected_row(), ManagerListRow::NewWorkspace)
    }

    /// The [`WorkspaceSummary`] currently highlighted, or `None` when the
    /// selection is on Current Directory or New Workspace.
    #[must_use]
    pub fn selected_workspace_summary(&self) -> Option<&WorkspaceSummary> {
        if let ManagerListRow::SavedWorkspace(i) = self.selected_row() {
            self.workspaces.get(i)
        } else {
            None
        }
    }

    /// Drained from the outer event loop every tick so picker results
    /// land without keystroke pumping. Idempotent on empty channels.
    /// Covers both modal anchors (`list_modal` and `editor.modal`).
    pub fn poll_picker_loads(&mut self) {
        if let Some(Modal::OpPicker { state }) = self.list_modal.as_mut() {
            state.poll_load();
        }
        if let ManagerStage::Editor(editor) = &mut self.stage
            && let Some(Modal::OpPicker { state }) = editor.modal.as_mut()
        {
            state.poll_load();
        }
    }
}

impl EditorState<'_> {
    pub fn new_edit(name: String, ws: WorkspaceConfig) -> Self {
        Self {
            mode: EditorMode::Edit { name },
            active_tab: EditorTab::General,
            active_field: FieldFocus::Row(0),
            original: ws.clone(),
            pending: ws,
            modal: None,
            pending_name: None,
            exit_after_save: None,
            save_flow: EditorSaveFlow::Idle,
            unmasked_rows: BTreeSet::default(),
            secrets_expanded: BTreeSet::default(),
            pending_env_key: None,
            pending_picker_target: None,
            pending_picker_value: None,
        }
    }

    pub fn new_create() -> Self {
        let empty = WorkspaceConfig::default();
        Self {
            mode: EditorMode::Create,
            active_tab: EditorTab::General,
            active_field: FieldFocus::Row(0),
            original: empty.clone(),
            pending: empty,
            modal: None,
            pending_name: None,
            exit_after_save: None,
            save_flow: EditorSaveFlow::Idle,
            unmasked_rows: BTreeSet::default(),
            secrets_expanded: BTreeSet::default(),
            pending_env_key: None,
            pending_picker_target: None,
            pending_picker_value: None,
        }
    }

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

    /// Field-level diff count used for "s save (N changes)".
    pub fn change_count(&self) -> usize {
        let mut n = 0;
        if self.pending.workdir != self.original.workdir {
            n += 1;
        }
        if self.pending.default_agent != self.original.default_agent {
            n += 1;
        }
        if self.pending.allowed_agents != self.original.allowed_agents {
            n += 1;
        }
        // Rename in Edit mode counts as a change.
        if let EditorMode::Edit { name } = &self.mode
            && self.pending_name.as_deref().is_some_and(|pn| pn != name)
        {
            n += 1;
        }
        // MountConfig has no Ord/Hash; linear contains is fine for
        // the few mounts a workspace has.
        let added = self
            .pending
            .mounts
            .iter()
            .filter(|m| !self.original.mounts.contains(m))
            .count();
        let removed = self
            .original
            .mounts
            .iter()
            .filter(|m| !self.pending.mounts.contains(m))
            .count();
        n += added + removed;
        n += env_change_count(&self.original.env, &self.pending.env);
        // Per-agent overrides: union the keys; an agent present on
        // only one side counts its whole env map as added/removed.
        let agent_keys: std::collections::BTreeSet<&String> = self
            .original
            .agents
            .keys()
            .chain(self.pending.agents.keys())
            .collect();
        for agent in agent_keys {
            let orig = self.original.agents.get(agent).map(|o| &o.env);
            let pend = self.pending.agents.get(agent).map(|p| &p.env);
            let empty = std::collections::BTreeMap::<String, String>::new();
            let orig_env = orig.unwrap_or(&empty);
            let pend_env = pend.unwrap_or(&empty);
            n += env_change_count(orig_env, pend_env);
        }
        n
    }
}

fn env_change_count(
    original: &std::collections::BTreeMap<String, String>,
    pending: &std::collections::BTreeMap<String, String>,
) -> usize {
    let mut n = 0;
    for (k, v) in pending {
        match original.get(k) {
            None => n += 1,                // added
            Some(ov) if ov != v => n += 1, // changed
            _ => {}
        }
    }
    for k in original.keys() {
        if !pending.contains_key(k) {
            n += 1; // removed
        }
    }
    n
}

impl Default for CreatePreludeState<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl CreatePreludeState<'_> {
    pub const fn new() -> Self {
        Self {
            step: CreateStep::PickFirstMountSrc,
            pending_mount_src: None,
            pending_mount_dst: None,
            pending_readonly: false,
            pending_workdir: None,
            pending_name: None,
            modal: None,
            last_browser_cwd: None,
            used_edit_dst: false,
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{MountConfig, WorkspaceConfig};

    fn empty_ws(workdir: &str) -> WorkspaceConfig {
        WorkspaceConfig {
            workdir: workdir.into(),
            ..Default::default()
        }
    }

    #[test]
    fn summary_counts_mounts_and_readonly() {
        let ws = WorkspaceConfig {
            workdir: "/a".into(),
            mounts: vec![
                MountConfig {
                    src: "/s1".into(),
                    dst: "/a".into(),
                    readonly: false,
                },
                MountConfig {
                    src: "/s2".into(),
                    dst: "/b".into(),
                    readonly: true,
                },
            ],
            allowed_agents: vec!["agent-smith".into()],
            ..Default::default()
        };
        let sum = WorkspaceSummary::from_config("big-monorepo", &ws);
        assert_eq!(sum.name, "big-monorepo");
        assert_eq!(sum.mount_count, 2);
        assert_eq!(sum.readonly_mount_count, 1);
        assert_eq!(sum.allowed_agent_count, 1);
    }

    #[test]
    fn manager_from_config_lists_all_workspaces() {
        let mut config = AppConfig::default();
        config.workspaces.insert("a".into(), empty_ws("/a"));
        // cwd is unrelated to /a — landing row is the synthetic
        // "Current directory" at index 0.
        let tmp = tempfile::tempdir().unwrap();
        let state = ManagerState::from_config(&config, tmp.path());
        assert_eq!(state.workspaces.len(), 1);
        assert!(matches!(state.stage, ManagerStage::List));
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn manager_preselects_saved_workspace_matching_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().canonicalize().unwrap();
        let workdir = project.display().to_string();

        let mut config = AppConfig::default();
        config.workspaces.insert(
            "big-monorepo".into(),
            WorkspaceConfig {
                workdir: workdir.clone(),
                mounts: vec![MountConfig {
                    src: workdir.clone(),
                    dst: workdir,
                    readonly: false,
                }],
                ..Default::default()
            },
        );
        // Second workspace that does NOT match cwd — used to verify the
        // preselect calculation points at the matching one, not simply
        // "index 1" which works for a single workspace by accident.
        config
            .workspaces
            .insert("z-unrelated".into(), empty_ws("/some/other/path"));

        let state = ManagerState::from_config(&config, &project);
        // Workspaces are ordered by BTreeMap key: ["big-monorepo", "z-unrelated"].
        // "big-monorepo" is at saved_index 0, so selected = 1 + 0 = 1.
        assert_eq!(state.selected, 1);
        assert_eq!(state.workspaces[state.selected - 1].name, "big-monorepo");
    }

    /// Pins that `ms.selected == 0` means "Current directory" regardless
    /// of how many saved workspaces are present. The render path
    /// (`render_list_body`) and the input path (`handle_list_key`) both
    /// depend on this: selected==0 is the synthetic cwd row, 1..=N are
    /// saved workspaces, N+1 is the "+ New workspace" sentinel.
    #[test]
    fn manager_current_directory_is_first_row() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().canonicalize().unwrap();

        // Empty config: only the synthetic "Current directory" + sentinel.
        let config_empty = AppConfig::default();
        let state_empty = ManagerState::from_config(&config_empty, &cwd);
        assert_eq!(state_empty.selected, 0);
        assert_eq!(state_empty.workspaces.len(), 0);

        // Non-empty config with unrelated saved workspaces — preselect
        // still lands on row 0.
        let mut config = AppConfig::default();
        config
            .workspaces
            .insert("a".into(), empty_ws("/some/other/path"));
        config
            .workspaces
            .insert("b".into(), empty_ws("/yet/another"));
        let state = ManagerState::from_config(&config, &cwd);
        assert_eq!(
            state.selected, 0,
            "selected==0 must always map to Current directory"
        );
        assert_eq!(state.workspaces.len(), 2);
    }

    #[test]
    fn manager_preselects_current_directory_when_no_saved_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().canonicalize().unwrap();

        let mut config = AppConfig::default();
        config
            .workspaces
            .insert("unrelated".into(), empty_ws("/some/other/path"));

        let state = ManagerState::from_config(&config, &cwd);
        assert_eq!(
            state.selected, 0,
            "no saved workspace covers cwd → land on Current directory"
        );
    }

    #[test]
    fn new_edit_is_not_dirty() {
        let e = EditorState::new_edit("a".into(), empty_ws("/a"));
        assert!(!e.is_dirty());
        assert_eq!(e.change_count(), 0);
    }

    #[test]
    fn changing_workdir_is_dirty_count_one() {
        let mut e = EditorState::new_edit("a".into(), empty_ws("/a"));
        e.pending.workdir = "/b".into();
        assert!(e.is_dirty());
        assert_eq!(e.change_count(), 1);
    }

    #[test]
    fn adding_mount_counts_as_one_change() {
        let mut e = EditorState::new_edit("a".into(), empty_ws("/a"));
        e.pending.mounts.push(MountConfig {
            src: "/s".into(),
            dst: "/a".into(),
            readonly: false,
        });
        assert_eq!(e.change_count(), 1);
    }

    // ── change_count env-diff coverage (Secrets tab) ──

    /// Setting a new workspace-level env key on `pending` (with
    /// `original.env` empty) contributes exactly +1 to the change count.
    #[test]
    fn change_count_env_set_counts_as_one() {
        let mut e = EditorState::new_edit("a".into(), empty_ws("/a"));
        assert_eq!(e.change_count(), 0);
        e.pending.env.insert("DB_URL".into(), "postgres://…".into());
        assert_eq!(e.change_count(), 1);
    }

    /// Removing an existing workspace-level env key (seeded in
    /// `original.env` at construction time) contributes exactly +1.
    #[test]
    fn change_count_env_remove_counts_as_one() {
        let mut ws = empty_ws("/a");
        ws.env.insert("DB_URL".into(), "postgres://…".into());
        let mut e = EditorState::new_edit("a".into(), ws);
        assert_eq!(e.change_count(), 0);
        e.pending.env.remove("DB_URL");
        assert_eq!(e.change_count(), 1);
    }

    /// Adding and removing per-agent env override keys each contribute +1
    /// via the same env_change_count helper as workspace-level env.
    #[test]
    fn change_count_agent_env_delta() {
        use crate::workspace::WorkspaceAgentOverride;
        // Seed one agent with one env key.
        let mut ws = empty_ws("/a");
        let mut agent_x_env = std::collections::BTreeMap::new();
        agent_x_env.insert("LOG_LEVEL".into(), "info".into());
        ws.agents.insert(
            "agent-x".into(),
            WorkspaceAgentOverride { env: agent_x_env },
        );
        let mut e = EditorState::new_edit("a".into(), ws);
        assert_eq!(e.change_count(), 0);

        // Add a new key to pending.
        e.pending
            .agents
            .get_mut("agent-x")
            .unwrap()
            .env
            .insert("DEBUG".into(), "1".into());
        assert_eq!(e.change_count(), 1);

        // Remove the original key. Net delta: 2 (one add + one remove).
        e.pending
            .agents
            .get_mut("agent-x")
            .unwrap()
            .env
            .remove("LOG_LEVEL");
        assert_eq!(e.change_count(), 2);
    }

    /// Any env mutation (workspace-level or per-agent) flips `is_dirty()`
    /// to true because `pending != original` in the underlying
    /// `WorkspaceConfig` PartialEq.
    #[test]
    fn is_dirty_from_env_mutation() {
        use crate::workspace::WorkspaceAgentOverride;

        // Workspace env path.
        let mut e = EditorState::new_edit("a".into(), empty_ws("/a"));
        assert!(!e.is_dirty());
        e.pending.env.insert("K".into(), "v".into());
        assert!(e.is_dirty(), "workspace env set must make state dirty");

        // Per-agent env path.
        let mut e2 = EditorState::new_edit("a".into(), empty_ws("/a"));
        assert!(!e2.is_dirty());
        e2.pending.agents.insert(
            "agent-x".into(),
            WorkspaceAgentOverride {
                env: {
                    let mut m = std::collections::BTreeMap::new();
                    m.insert("K".into(), "v".into());
                    m
                },
            },
        );
        assert!(e2.is_dirty(), "agent env set must make state dirty");
    }

    #[test]
    fn create_prelude_starts_at_first_step() {
        let p = CreatePreludeState::new();
        assert!(matches!(p.step, CreateStep::PickFirstMountSrc));
    }

    // ── completed() helper — keeps name+ws invariants in lockstep ──

    #[test]
    fn completed_returns_none_when_name_missing() {
        let mut p = CreatePreludeState::new();
        p.accept_mount_src(PathBuf::from("/home/user/proj"));
        p.accept_mount_dst("/home/user/proj".into(), false);
        p.accept_workdir("/home/user/proj".into());
        // No accept_name → completed() must be None.
        assert!(p.completed().is_none());
    }

    #[test]
    fn completed_returns_none_when_mount_src_missing() {
        let mut p = CreatePreludeState::new();
        // Skip accept_mount_src and accept_mount_dst.
        p.pending_workdir = Some("/home/user/proj".into());
        p.pending_name = Some("proj".into());
        // build_workspace fails on missing src → completed() None.
        assert!(p.completed().is_none());
    }

    #[test]
    fn completed_returns_none_when_workdir_missing() {
        let mut p = CreatePreludeState::new();
        p.accept_mount_src(PathBuf::from("/home/user/proj"));
        p.accept_mount_dst("/home/user/proj".into(), false);
        // Skip accept_workdir.
        p.pending_name = Some("proj".into());
        assert!(p.completed().is_none());
    }

    #[test]
    fn completed_returns_some_when_all_fields_present() {
        let mut p = CreatePreludeState::new();
        p.accept_mount_src(PathBuf::from("/home/user/proj"));
        p.accept_mount_dst("/home/user/proj".into(), false);
        p.accept_workdir("/home/user/proj".into());
        p.accept_name("proj".into());
        let (name, ws) = p.completed().expect("all fields present");
        assert_eq!(name, "proj");
        assert_eq!(ws.workdir, "/home/user/proj");
        assert_eq!(ws.mounts.len(), 1);
        assert_eq!(ws.mounts[0].src, "/home/user/proj");
    }

    /// Pin the enum contract: round-tripping a `ManagerListRow` through
    /// `to_screen_index` / `row_at` / `selected_row` must yield the same
    /// logical row. Covers the three variants over a non-trivial saved set.
    #[test]
    fn manager_list_row_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        config.workspaces.insert("a".into(), empty_ws("/a"));
        config.workspaces.insert("b".into(), empty_ws("/b"));
        config.workspaces.insert("c".into(), empty_ws("/c"));
        let mut state = ManagerState::from_config(&config, cwd);

        let saved_count = state.workspaces.len();
        assert_eq!(state.row_count(), saved_count + 2);
        assert_eq!(state.new_workspace_row_index(), saved_count + 1);

        let rows = [
            ManagerListRow::CurrentDirectory,
            ManagerListRow::SavedWorkspace(0),
            ManagerListRow::SavedWorkspace(1),
            ManagerListRow::SavedWorkspace(2),
            ManagerListRow::NewWorkspace,
        ];
        for row in rows {
            let idx = row.to_screen_index(saved_count);
            assert_eq!(state.row_at(idx), Some(row), "row_at({idx}) for {row:?}");
            state.selected = idx;
            assert_eq!(state.selected_row(), row, "selected_row for idx={idx}");
        }

        // Out-of-range index returns None.
        assert_eq!(state.row_at(saved_count + 2), None);
    }

    /// `selected_workspace_summary` must return `None` for both synthetic
    /// rows (cwd + sentinel) and `Some(&WorkspaceSummary)` for a real
    /// saved row.
    #[test]
    fn manager_selected_workspace_summary_is_none_for_synthetic_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        config.workspaces.insert("alpha".into(), empty_ws("/alpha"));
        let mut state = ManagerState::from_config(&config, cwd);

        // Current directory row.
        state.selected = ManagerListRow::CurrentDirectory.to_screen_index(1);
        assert!(state.selected_workspace_summary().is_none());
        assert!(state.is_current_dir_selected());

        // Saved workspace row.
        state.selected = ManagerListRow::SavedWorkspace(0).to_screen_index(1);
        let summary = state
            .selected_workspace_summary()
            .expect("saved row exposes summary");
        assert_eq!(summary.name, "alpha");

        // "+ New workspace" sentinel.
        state.selected = ManagerListRow::NewWorkspace.to_screen_index(1);
        assert!(state.selected_workspace_summary().is_none());
        assert!(state.is_new_workspace_selected());
    }
}
