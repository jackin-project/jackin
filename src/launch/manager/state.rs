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
    DiscardChanges,
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
    pub fn from_config(config: &AppConfig) -> Self {
        let workspaces: Vec<WorkspaceSummary> = config
            .workspaces
            .iter()
            .map(|(name, ws)| WorkspaceSummary::from_config(name, ws))
            .collect();
        Self {
            stage: ManagerStage::List,
            workspaces,
            selected: 0,
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
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.pending != self.original
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
        let state = ManagerState::from_config(&config);
        assert_eq!(state.workspaces.len(), 1);
        assert!(matches!(state.stage, ManagerStage::List));
        assert_eq!(state.selected, 0);
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
