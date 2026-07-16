// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Non-UI launch cockpit value types: stages, identity, failure, restore
//! dialog data, and port traits. Shared by the orchestration layer
//! (`jackin-runtime`) and the presentation layer (`jackin-launch`) with no
//! dependency on Ratatui or a presentation crate.

use std::future::Future;
use std::path::{Path, PathBuf};

// --- Stage types ---

/// Ordered stages of the launch cockpit pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum LaunchStage {
    /// Resolve instance identity / selector.
    Identity,
    /// Load and validate the role definition.
    Role,
    /// Resolve and inject credentials.
    Credentials,
    /// Construct runtime plan / container spec.
    Construct,
    /// Prefetch or stage agent CLI binaries.
    AgentBinaries,
    /// Build or select the derived role image.
    DerivedImage,
    /// Prepare workspace mounts and isolation.
    Workspace,
    /// Create or attach Docker networks.
    Network,
    /// Start sidecar containers if any.
    Sidecar,
    /// Start the capsule / agent container.
    Capsule,
    /// Establish the hardline attach path.
    Hardline,
}

impl LaunchStage {
    /// Every stage in pipeline order.
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

    /// Human-readable stage label for the cockpit UI.
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

/// Per-stage progress status in the launch cockpit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageStatus {
    /// Not started yet.
    Queued,
    /// Currently executing.
    Running,
    /// Completed successfully.
    Done,
    /// Intentionally skipped for this launch.
    Skipped,
    /// Failed; launch aborted or blocked.
    Failed,
    /// Waiting on an earlier failed/blocked stage.
    Blocked,
}

/// Renderable snapshot of one stage row.
#[derive(Debug, Clone)]
pub struct StageView {
    /// Which stage this row represents.
    pub stage: LaunchStage,
    /// Current status.
    pub status: StageStatus,
    /// Optional detail line under the stage label.
    pub detail: String,
}

/// Animated label transition between stage indices.
#[derive(Debug, Clone, Copy)]
pub struct StageLabelTransition {
    /// Source stage index in [`LaunchStage::ALL`].
    pub from: usize,
    /// Destination stage index in [`LaunchStage::ALL`].
    pub to: usize,
    /// Animation frame when the transition started.
    pub start_frame: usize,
}

// --- Launch identity and failure ---

/// Identity header shown at the top of the launch cockpit.
#[derive(Debug, Clone)]
pub struct LaunchIdentity {
    /// Role key / name.
    pub role: String,
    /// Agent slug / label.
    pub agent: String,
    /// Whether launch targets a named workspace or a raw directory.
    pub target_kind: LaunchTargetKind,
    /// Workspace or directory label for display.
    pub target_label: String,
    /// Mounts whose host source differs from the container destination,
    /// pre-formatted for display. Same-path mounts are omitted upstream.
    pub mounts: Vec<String>,
    /// Derived image reference when known.
    pub image: Option<String>,
    /// Container name when known.
    pub container: Option<String>,
}

/// Structured launch failure payload for the failure surface.
#[derive(Debug, Clone)]
pub struct LaunchFailure {
    /// Short failure title.
    pub title: String,
    /// One-line summary.
    pub summary: String,
    /// Optional multi-line detail.
    pub detail: Option<String>,
    /// Suggested next step for the operator.
    pub next_step: Option<String>,
    /// Stage where the failure occurred.
    pub stage: LaunchStage,
    /// Path to the diagnostics bundle when written.
    pub diagnostics_path: Option<PathBuf>,
    /// Path to captured command output when written.
    pub command_output_path: Option<PathBuf>,
}

/// Kind of launch target shown in identity copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchTargetKind {
    /// Named workspace from config.
    Workspace,
    /// Ad-hoc host directory.
    Directory,
}

impl LaunchTargetKind {
    /// Preposition phrase used in launch copy (`"into workspace"` / `"in directory"`).
    #[must_use]
    pub const fn launch_preposition(self) -> &'static str {
        match self {
            Self::Workspace => "into workspace",
            Self::Directory => "in directory",
        }
    }
}

/// Clipboard targets offered on the failure surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCopyTarget {
    /// Copy the diagnostics run id.
    RunId,
    /// Copy the diagnostics directory path.
    DiagnosticsPath,
    /// Copy the command-output file path.
    CommandOutputPath,
}

// --- Prompt context ---

/// One styled line in a prompt/dialog context block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptContextLine {
    /// Emphasized body text.
    Emphasis(String),
    /// Dim/muted body text.
    Muted(String),
    /// Path-styled text.
    Path(String),
    /// Unstyled plain text.
    Plain(String),
    /// Empty spacer line.
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

/// Diagnostics sink for a single launch run (paths, compact log, stage events).
pub trait LaunchDiagnostics: Send + Sync {
    /// Stable run identifier for this launch.
    fn run_id(&self) -> &str;
    /// Root diagnostics directory for this run.
    fn path(&self) -> &Path;
    /// Whether diagnostics are being persisted to disk.
    fn persists(&self) -> bool;
    /// Path for a named command-output capture file.
    fn command_output_path(&self, name: &str) -> PathBuf;
    /// Emit a compact always-on log line.
    fn compact(&self, kind: &str, message: &str);
    /// Emit an error line with optional structured error type.
    fn error(&self, kind: &str, message: &str, error_type: Option<&str>);
    /// Emit a stage transition / progress event.
    fn stage(&self, kind: &str, stage: LaunchStage, message: &str, detail: Option<&str>);
}

/// Host terminal side-effects available during launch (clipboard, pointer, debug).
pub trait LaunchHostTerminal: Send + Sync {
    /// Mark whether a rich TUI surface currently owns the terminal.
    fn set_rich_surface_active(&self, active: bool);
    /// Whether the host already owns the alternate screen.
    fn host_screen_owned(&self) -> bool;
    /// Whether verbose debug logging is enabled for this process.
    fn is_debug_mode(&self) -> bool;
    /// Emit a compact operator-facing line.
    fn emit_compact_line(&self, kind: &str, line: &str);
    /// Emit a debug-category line when debug mode is on.
    fn emit_debug_line(&self, category: &str, line: &str);
    /// Toggle OSC 22 pointer shape (`true` = hand/pointer).
    fn set_pointer_shape(&self, pointer: bool);
    /// Copy `payload` to the system clipboard; returns success.
    fn copy_to_clipboard(&self, payload: &str) -> bool;
    /// Reveal `path` in the host file manager; returns success.
    fn reveal_file(&self, path: &Path) -> bool;
    /// Open `path` with the host default handler; returns success.
    fn open_file(&self, path: &Path) -> bool;
}

/// Port for launch-phase terminal side-effects (deploy banner, failure
/// lines, warp outro animations). Lives in core so `jackin-runtime` can
/// call without depending on a presentation crate. Implemented by an adapter in
/// `jackin-launch` and injected via static accessor (mirrors
/// [`LaunchHostTerminal`] / `host_terminal`).
pub trait LaunchOutputSink: Send + Sync {
    /// Animated "deploying" banner for `role_name`.
    fn print_deploying<'a>(
        &'a self,
        role_name: &'a str,
    ) -> std::pin::Pin<Box<dyn Future<Output = ()> + 'a>>;
    /// Print a step-failure line.
    fn step_fail(&self, msg: &str);
    /// Warp-out animation when leaving the host screen.
    fn warp_out(&self, host_screen_owned: bool);
    /// End-of-warp caption with optional elapsed duration.
    fn warp_end_caption(&self, elapsed: Option<std::time::Duration>, host_screen_owned: bool);
}
