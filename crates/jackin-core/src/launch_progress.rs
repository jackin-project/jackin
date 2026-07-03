//! Non-UI launch cockpit value types: stages, identity, failure, restore
//! dialog data, and port traits. Shared by the orchestration layer
//! (`jackin-runtime`) and the presentation layer (`jackin-launch-tui`) with no
//! dependency on `ratatui` or `jackin-tui`.

use std::future::Future;
use std::path::{Path, PathBuf};

// --- Stage types ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum LaunchStage {
    Identity,
    Role,
    Credentials,
    Construct,
    AgentBinaries,
    DerivedImage,
    Workspace,
    Network,
    Sidecar,
    Capsule,
    Hardline,
}

impl LaunchStage {
    pub const ALL: [Self; 11] = [
        Self::Identity,
        Self::Role,
        Self::Credentials,
        Self::Construct,
        Self::AgentBinaries,
        Self::DerivedImage,
        Self::Workspace,
        Self::Network,
        Self::Sidecar,
        Self::Capsule,
        Self::Hardline,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Role => "role",
            Self::Credentials => "credentials",
            Self::Construct => "construct",
            Self::AgentBinaries => "agent binaries",
            Self::DerivedImage => "derived image",
            Self::Workspace => "workspace",
            Self::Network => "network",
            Self::Sidecar => "sidecar",
            Self::Capsule => "capsule",
            Self::Hardline => "hardline",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageStatus {
    Queued,
    Running,
    Done,
    Skipped,
    Failed,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct StageView {
    pub stage: LaunchStage,
    pub status: StageStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Copy)]
pub struct StageLabelTransition {
    pub from: usize,
    pub to: usize,
    pub start_frame: usize,
}

// --- Launch identity and failure ---

#[derive(Debug, Clone)]
pub struct LaunchIdentity {
    pub role: String,
    pub agent: String,
    pub target_kind: LaunchTargetKind,
    pub target_label: String,
    /// Mounts whose host source differs from the container destination,
    /// pre-formatted for display. Same-path mounts are omitted upstream.
    pub mounts: Vec<String>,
    pub image: Option<String>,
    pub container: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LaunchFailure {
    pub title: String,
    pub summary: String,
    pub detail: Option<String>,
    pub next_step: Option<String>,
    pub stage: LaunchStage,
    pub diagnostics_path: Option<PathBuf>,
    pub command_output_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchTargetKind {
    Workspace,
    Directory,
}

impl LaunchTargetKind {
    #[must_use]
    pub const fn launch_preposition(self) -> &'static str {
        match self {
            Self::Workspace => "into workspace",
            Self::Directory => "in directory",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCopyTarget {
    RunId,
    DiagnosticsPath,
    CommandOutputPath,
}

// --- Prompt context ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptContextLine {
    Emphasis(String),
    Muted(String),
    Path(String),
    Plain(String),
    Blank,
}

// --- Restore dialog types ---

/// One changed file entry for the D24 Inspect surface.
#[derive(Debug, Clone)]
pub struct FileDiff {
    /// Porcelain status character (`M`, `A`, `D`, `?`, …).
    pub status: char,
    /// Path relative to the worktree root.
    pub path: String,
    /// File content at HEAD — `None` for added/untracked files.
    pub before: Option<String>,
    /// File content in the working tree — `None` for deleted files.
    pub after: Option<String>,
}

/// Pre-computed inspection data for one worktree in the D24 surface.
#[derive(Debug, Clone)]
pub struct WorktreeInspect {
    /// Display label shown in the repos pane (workspace name or mount path).
    pub label: String,
    /// Changed files with their diff content.
    pub files: Vec<FileDiff>,
}

/// One candidate row in the D23 launch dialog.
#[derive(Debug, Clone)]
pub struct LaunchCandidate {
    /// Formatted label shown in the picker list.
    pub label: String,
    /// `true` if the candidate has dirty/unpushed state.
    /// Dirty candidates require a `ConfirmDialog` before deletion (D21).
    pub is_dirty: bool,
    /// Pre-fetched inspect data (one entry per isolated worktree in this
    /// instance). Empty for clean/crashed instances with no worktree state.
    pub inspect: Vec<WorktreeInspect>,
}

/// Outcome of the D23 launch dialog.
#[derive(Debug, Clone)]
pub enum LaunchDialogResult {
    /// Operator chose to start a new instance.
    StartFresh,
    /// Operator chose to restore the candidate at this index.
    Restore(usize),
    /// Operator confirmed deletion of the candidate at this index.
    Delete(usize),
}

// --- Cancellation marker ---

/// Marker error: the operator deliberately aborted the launch (Ctrl+C,
/// Ctrl+Q, or a Cancel modal). This is an intent, not a failure — the binary
/// entry point treats it as a clean exit and never renders it as `error:`.
///
/// Carried as a concrete error inside an `anyhow::Error` so any layer can
/// detect it via [`LaunchCancelled::is_cancel`] regardless of `.context(..)`
/// wrapping. `Display` keeps the historical "launch cancelled by operator"
/// wording for debug/log surfaces.
#[derive(Debug)]
pub struct LaunchCancelled;

impl std::fmt::Display for LaunchCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("launch cancelled by operator")
    }
}

impl std::error::Error for LaunchCancelled {}

impl LaunchCancelled {
    /// Build the cancellation as an `anyhow::Error` for return up the stack.
    pub fn err() -> anyhow::Error {
        anyhow::Error::new(Self)
    }

    /// `true` if `error` — or anything in its source chain — is a
    /// `LaunchCancelled`. `anyhow`'s downcast walks the chain, so the check
    /// survives intermediate `.context(..)` layers.
    pub fn is_cancel(error: &anyhow::Error) -> bool {
        error.downcast_ref::<Self>().is_some()
    }
}

// --- Port traits ---

pub trait LaunchDiagnostics: Send + Sync {
    fn run_id(&self) -> &str;
    fn path(&self) -> &Path;
    fn persists(&self) -> bool {
        true
    }
    fn command_output_path(&self, name: &str) -> PathBuf;
    fn compact(&self, kind: &str, message: &str);
    fn error(&self, kind: &str, message: &str, detail: Option<&str>);
    fn stage(&self, kind: &str, stage: &str, message: &str, detail: Option<&str>);
}

pub trait LaunchHostTerminal: Send + Sync {
    fn set_rich_surface_active(&self, active: bool);
    fn host_screen_owned(&self) -> bool;
    fn is_debug_mode(&self) -> bool;
    fn emit_compact_line(&self, kind: &str, line: &str);
    fn emit_debug_line(&self, category: &str, line: &str);
    fn set_pointer_shape(&self, pointer: bool);
    fn copy_to_clipboard(&self, payload: &str) -> bool;
    fn reveal_file(&self, path: &Path) -> bool;
    fn open_file(&self, path: &Path) -> bool;
}

/// Port for launch-phase terminal side-effects (deploy banner, failure
/// lines, warp outro animations). Lives in core so `jackin-runtime` can
/// call without depending on `jackin-tui`. Implemented by an adapter in
/// `jackin-launch-tui` and injected via static accessor (mirrors
/// `LaunchHostTerminal` / `host_terminal`).
pub trait LaunchOutputSink: Send + Sync {
    fn print_deploying<'a>(
        &'a self,
        role_name: &'a str,
    ) -> std::pin::Pin<Box<dyn Future<Output = ()> + 'a>>;
    fn step_fail(&self, msg: &str);
    fn warp_out(&self, host_screen_owned: bool);
    fn warp_end_caption(&self, elapsed: Option<std::time::Duration>, host_screen_owned: bool);
}
