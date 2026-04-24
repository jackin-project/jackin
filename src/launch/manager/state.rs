//! Manager state machine. See docs/superpowers/specs/2026-04-23-workspace-manager-tui-design.md § 3.

use std::path::PathBuf;

use crate::config::AppConfig;
use crate::workspace::WorkspaceConfig;

use crate::launch::widgets::{
    confirm::ConfirmState, file_browser::FileBrowserState, text_input::TextInputState,
    workdir_pick::WorkdirPickState,
};

#[derive(Debug)]
pub struct ManagerState<'a> {
    pub stage: ManagerStage<'a>,
    pub workspaces: Vec<WorkspaceSummary>,
    pub selected: usize,
    pub toast: Option<Toast>,
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
    pub error_banner: Option<String>,
    /// In Create mode, the workspace name the prelude collected.
    /// Unused in Edit mode (name comes from `EditorMode::Edit { name }`).
    pub pending_name: Option<String>,
    /// Set by the `SaveDiscardCancel` modal handler to signal that the outer
    /// `handle_key` should perform a save and/or navigate to List.
    pub exit_after_save: Option<ExitIntent>,
    /// Set to `true` when the operator has confirmed a mount-collapse plan
    /// via the `ConfirmTarget::SaveCollapse` modal. Tells `save_editor` to
    /// skip the "should I prompt?" check and proceed to write. Cleared on
    /// every save path exit.
    pub collapse_approved: bool,
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
    WorkdirPick {
        state: WorkdirPickState,
    },
    Confirm {
        target: ConfirmTarget,
        state: ConfirmState,
    },
    SaveDiscardCancel {
        state: crate::launch::widgets::save_discard::SaveDiscardState,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputTarget {
    Name,
    Workdir,
    MountDst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileBrowserTarget {
    CreateFirstMountSrc,
    EditAddMountSrc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmTarget {
    DeleteWorkspace,
    /// Operator must confirm the plan returned by `plan_edit`/`plan_create`
    /// will collapse one or more redundant mounts before save writes.
    SaveCollapse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitIntent {
    Save,
    Discard,
    /// Operator approved a mount-collapse plan. Re-run `save_editor` in
    /// place — stay in the editor on success (mirroring a normal `s`
    /// press) rather than transitioning to the workspace list.
    RetrySave,
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
    /// Row layout in the manager list (mirrored in `render_list_body` and
    /// `handle_list_key`):
    ///
    ///   row 0              → synthetic "Current directory" choice
    ///   rows 1..=N         → saved workspaces, in `BTreeMap` order
    ///   row N+1            → "+ New workspace" sentinel
    ///
    /// When cwd is covered by a saved workspace, preselect the saved row
    /// (index = 1 + its position in config.workspaces). Otherwise land on
    /// row 0 so Enter launches against the current directory without saving.
    pub fn from_config(config: &AppConfig, cwd: &std::path::Path) -> Self {
        let workspaces: Vec<WorkspaceSummary> = config
            .workspaces
            .iter()
            .map(|(name, ws)| WorkspaceSummary::from_config(name, ws))
            .collect();

        let selected = crate::app::context::find_saved_workspace_for_cwd(config, cwd)
            .and_then(|(name, _)| workspaces.iter().position(|w| w.name == name))
            .map_or(0, |idx| idx + 1);

        Self {
            stage: ManagerStage::List,
            workspaces,
            selected,
            toast: None,
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
            error_banner: None,
            pending_name: None,
            exit_after_save: None,
            collapse_approved: false,
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
            error_banner: None,
            pending_name: None,
            exit_after_save: None,
            collapse_approved: false,
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
        n
    }
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
}
