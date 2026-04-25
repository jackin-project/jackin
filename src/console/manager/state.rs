//! Manager state machine. See docs/superpowers/specs/2026-04-23-workspace-manager-tui-design.md § 3.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::config::AppConfig;
use crate::workspace::WorkspaceConfig;

use crate::console::widgets::{
    confirm::ConfirmState, confirm_save::ConfirmSaveState, error_popup::ErrorPopupState,
    file_browser::FileBrowserState, github_picker::GithubPickerState,
    mount_dst_choice::MountDstChoiceState, text_input::TextInputState,
    workdir_pick::WorkdirPickState,
};

/// Logical identity of a row in the workspace-manager list.
///
/// The list has a fixed shape:
///   - `CurrentDirectory` — the synthetic "Current directory" row (always row 0 on screen)
///   - `SavedWorkspace(i)` — the i-th saved workspace (0-indexed into `ManagerState::workspaces`)
///   - `NewWorkspace`     — the "+ New workspace" sentinel (always the last row on screen)
///
/// Prefer this enum (and the `ManagerState` helpers below) over the raw
/// `selected: usize` when reasoning about what the operator is pointing at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerListRow {
    CurrentDirectory,
    SavedWorkspace(usize),
    NewWorkspace,
}

impl ManagerListRow {
    /// Inverse of [`ManagerState::row_at`]. Maps a logical row back to the
    /// raw screen index — used by callers that still need to hand a `usize`
    /// to ratatui's [`ListState::select`].
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
    /// Modal overlay anchored at the `ManagerState` level — populated only
    /// from the list view (e.g. `Modal::GithubPicker`). The Editor and
    /// `CreatePrelude` stages own their own modal slots on their inner state.
    pub list_modal: Option<Modal<'a>>,
    /// Left-pane (workspace list) width as percentage of total terminal
    /// width. Clamped to [`MIN_SPLIT_PCT`, `MAX_SPLIT_PCT`]. Drives the
    /// 30/70 split in `render_list_body`; mouse-drag on the seam column
    /// updates it via `handle_mouse`.
    pub list_split_pct: u16,
    /// Active mouse-drag on the list/details seam. `Some` while a left
    /// button is held down after a seam-anchored Down event; cleared on
    /// Up. Readers (render) never need this — only the mouse handler.
    pub drag_state: Option<DragState>,
}

/// Anchors a mouse-drag resize of the list/details seam.
///
/// Captured on `MouseEventKind::Down(Left)` when the click lands within
/// ±1 column of the current seam; consumed by `MouseEventKind::Drag(Left)`
/// events to compute the new `list_split_pct`; cleared on `Up`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragState {
    /// `list_split_pct` at the moment the drag began.
    pub anchor_pct: u16,
    /// Mouse column (0-based) at the moment the drag began.
    pub anchor_x: u16,
}

/// Minimum list-pane width as percentage of total terminal width.
///
/// Keeps the workspace-name list readable even when operator drags the
/// seam left. Mirrors the cap on the details pane (`100 - MAX_SPLIT_PCT`
/// is 20).
pub const MIN_SPLIT_PCT: u16 = 20;
/// Maximum list-pane width as percentage of total terminal width. Keeps
/// the details pane viable when the operator drags the seam right.
pub const MAX_SPLIT_PCT: u16 = 80;
/// Initial split value — gives workspace names a tight column and lets
/// the details pane breathe for git branches and full paths.
pub const DEFAULT_SPLIT_PCT: u16 = 30;

/// Clamp `pct` into the allowed [`MIN_SPLIT_PCT`, `MAX_SPLIT_PCT`] range.
/// Used by `handle_mouse` to keep drag-computed percentages sane, and by
/// `ManagerState::from_config` for defense in depth.
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
    /// In Create mode, the workspace name the prelude collected.
    /// Unused in Edit mode (name comes from `EditorMode::Edit { name }`).
    pub pending_name: Option<String>,
    /// Set by the `SaveDiscardCancel` modal handler to signal that the outer
    /// `handle_key` should perform a save and/or navigate to List.
    pub exit_after_save: Option<ExitIntent>,
    /// Explicit state machine for the multi-step save protocol (open
    /// `ConfirmSave` → stash plan → outer-loop commit → maybe `ErrorPopup`).
    /// Replaces the sibling `error_banner`, `exit_on_save_success`, and
    /// `pending_save_commit` flags that used to live on this struct.
    pub save_flow: EditorSaveFlow,
    /// Secrets tab: whether values are rendered masked (default `true`).
    /// Toggled via `Ctrl+M` while on the Secrets tab. Resets to `true`
    /// each time the operator leaves and re-enters the tab.
    pub secrets_masked: bool,
    /// Secrets tab: which per-agent override sections are currently
    /// expanded. Keyed by agent name. Empty on tab entry; populated as
    /// the operator presses `→` / `Enter` on an agent header.
    pub secrets_expanded: BTreeSet<String>,
    /// Secrets tab: scratch field for the two-step "add" flow. Set when
    /// the first `TextInput(EnvKey)` modal commits; cleared when the
    /// follow-up `TextInput(EnvValue)` modal commits or cancels.
    pub pending_env_key: Option<(SecretsScopeTag, String)>,
}

/// Explicit state-machine for the workspace editor's save cycle.
///
/// The save path is a multi-step protocol:
///
/// 1. Operator presses `s` (or picks `Save` in the `SaveDiscardCancel` modal).
/// 2. `begin_editor_save` runs validation + planning and opens the
///    `ConfirmSave` modal — transitions `Idle → Confirming`.
/// 3. Operator picks `Save` in `ConfirmSave`: the modal handler stashes
///    the plan — transitions `Confirming → PendingCommit`.
/// 4. The outer `handle_key` (which holds `paths` / `cwd`) drains the
///    `PendingCommit` variant and calls `commit_editor_save`, which writes
///    to disk — success transitions back to `Idle`; failure transitions
///    to `Error` which renders as an `ErrorPopup` until the operator
///    dismisses it.
///
/// Validation failures from `begin_editor_save` (missing name, planner
/// reject, pre-existing-only collapse) also land in `Error`, with the
/// message surfaced as an inline banner instead of a modal — see the
/// rendering in `render_editor`.
#[derive(Debug, Clone, Default)]
pub enum EditorSaveFlow {
    #[default]
    Idle,
    /// Operator has opened the `ConfirmSave` modal; when they click Save,
    /// the modal handler stashes the commit plan and closes the modal.
    /// `exit_on_success` is true when this save cycle originated from
    /// `SaveDiscardCancel`'s Save choice (so the outer loop should pop
    /// back to the workspace list on success), false when the operator
    /// triggered save directly from the editor (stay in place).
    Confirming { exit_on_success: bool },
    /// `ConfirmSave` handler has handed off a ready-to-commit plan.
    /// Drained by the outer `handle_key` which actually performs the
    /// write (it holds `paths` / `cwd`).
    PendingCommit {
        plan: PendingSaveCommit,
        exit_on_success: bool,
    },
    /// The last commit attempt failed; `message` is shown in the
    /// `ErrorPopup` overlay (or, for pre-commit validation errors, as
    /// an inline banner) until dismissed.
    Error { message: String },
}

impl EditorSaveFlow {
    /// True when the save flow is in the `Error` state — used by the
    /// render path to decide whether to draw the inline banner.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    /// The error message currently pending display, if any.
    #[must_use]
    pub const fn error_message(&self) -> Option<&str> {
        if let Self::Error { message } = self {
            Some(message.as_str())
        } else {
            None
        }
    }
}

/// Plan material the `ConfirmSave` modal stashes on the editor state when
/// the operator clicks Save. `input.rs::commit_editor_save` drains this
/// and actually writes to disk.
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
    /// Opened from the workspace list view when the highlighted workspace
    /// has ≥2 GitHub mounts and the operator presses `o`. Committing picks
    /// one URL to hand to `open::that_detached`.
    GithubPicker {
        state: GithubPickerState,
    },
    /// Preview-and-confirm modal shown when the operator presses `s` in
    /// the editor with changes pending. Lists every field-level change
    /// (and any mount-collapse warning) up-front so the operator sees a
    /// single dialog covering the whole plan.
    ConfirmSave {
        state: ConfirmSaveState,
    },
    /// Error popup opened by the save path when an internal-API call
    /// returns an Err (e.g. duplicate workspace name, planner reject).
    /// Dismiss returns the operator to the editor with changes intact.
    ErrorPopup {
        state: ErrorPopupState,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInputTarget {
    Name,
    Workdir,
    MountDst,
    /// Secrets tab — entering the key half of the two-step add flow.
    /// Commit of this modal stashes the key on `pending_env_key` and
    /// opens a follow-up `EnvValue` modal.
    EnvKey {
        scope: SecretsScopeTag,
    },
    /// Secrets tab — entering the value, either editing an existing
    /// key (`scope` resolved from the focused row) or completing the
    /// second half of the two-step add flow.
    EnvValue {
        scope: SecretsScopeTag,
        key: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileBrowserTarget {
    CreateFirstMountSrc,
    EditAddMountSrc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmTarget {
    DeleteWorkspace,
    /// Secrets tab — confirming deletion of an env var. Carries the
    /// scope (workspace-level or agent override) and the key to remove.
    DeleteEnvVar {
        scope: SecretsScopeTag,
        key: String,
    },
}

/// Identifies a Secrets-tab scope when stashed in `TextInputTarget` /
/// `ConfirmTarget`.
///
/// Intentionally separate from [`crate::config::editor::EnvScope`] —
/// that type requires the workspace name, which `EditorState` doesn't
/// always have handy at modal-commit time (Create mode captures the name
/// on `pending_name`). We derive the full `EnvScope` at save time inside
/// `commit_editor_save`.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Last cwd the `FileBrowser` was pointing at when the operator
    /// committed a mount src. Captured so that pressing Esc on the
    /// `MountDstChoice` step can re-open `FileBrowser` at the same directory
    /// instead of starting back at `$HOME`.
    pub last_browser_cwd: Option<PathBuf>,
    /// Tracks whether the operator took the "Edit destination" branch
    /// during the current wizard run. Determines which step Esc on
    /// `WorkdirPick` should rewind to — `TextInputDst` if Edit was used,
    /// `MountDstChoice` otherwise.
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
    /// Build the manager state from config, preselecting the row that best
    /// matches `cwd`.
    ///
    /// See [`ManagerListRow`] docs for row layout.
    ///
    /// When cwd is covered by a saved workspace, preselect the saved row.
    /// Otherwise land on the current-directory row so Enter launches against
    /// the current directory without saving.
    pub fn from_config(config: &AppConfig, cwd: &std::path::Path) -> Self {
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
            secrets_masked: true,
            secrets_expanded: BTreeSet::default(),
            pending_env_key: None,
        }
    }

    pub fn new_create() -> Self {
        let empty = WorkspaceConfig {
            workdir: String::new(),
            mounts: vec![],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::default(),
            agents: std::collections::BTreeMap::default(),
        };
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
            secrets_masked: true,
            secrets_expanded: BTreeSet::default(),
            pending_env_key: None,
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

    /// Count field-level differences. Used for "s save (N changes)".
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
        // Mounts: count adds + removes + content changes.
        // MountConfig doesn't implement Ord/Hash so we use linear containment
        // checks — mount lists are small so this is perfectly acceptable.
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
        // Env vars: count workspace-level adds, removes, and value changes.
        n += env_change_count(&self.original.env, &self.pending.env);
        // Per-agent env overrides: union of agent keys across original
        // and pending. For each agent, count env deltas; if an agent
        // exists in one side but not the other, its whole env map is
        // counted as added/removed.
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

/// Count add/remove/change deltas between two env maps. Added keys,
/// removed keys, and value-changed keys each contribute +1.
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
            mounts: vec![],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: Default::default(),
            agents: Default::default(),
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
            default_agent: None,
            last_agent: None,
            env: Default::default(),
            agents: Default::default(),
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
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
                env: Default::default(),
                agents: Default::default(),
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
