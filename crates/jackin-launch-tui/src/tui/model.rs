//! Launch cockpit model types shared with runtime orchestration.

use std::path::PathBuf;

use jackin_tui::components::StatusFooterHover;
use ratatui::text::Line;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptContextLine {
    Emphasis(String),
    Muted(String),
    Path(String),
    Plain(String),
    Blank,
}

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

#[derive(Debug, Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "tracked in codebase-health-enforcement"
)]
pub struct LaunchView {
    pub identity: Option<LaunchIdentity>,
    pub stages: Vec<StageView>,
    pub status: String,
    pub failure: Option<LaunchFailure>,
    /// Operator dismissed the failure popup (Enter/Esc). The render task owns
    /// input, so it sets this flag; `LaunchProgress::stage_failed` awaits it
    /// rather than reading stdin itself.
    pub failure_ack: bool,
    pub frame: usize,
    /// Operator opened the live docker-build log overlay.
    pub build_log_open: bool,
    /// Lines scrolled up from the tail of the build log (0 = follow newest).
    pub build_log_scroll: jackin_tui::scroll::TailScroll,
    /// Pointer drag is currently bound to the build-log scrollbar track.
    pub build_log_scroll_dragging: bool,
    /// Render-safe snapshot of retained docker-build output.
    pub build_log_lines: Vec<String>,
    /// Wrapped docker-build output for the current dialog viewport width.
    pub build_log_wrapped_lines: Vec<Line<'static>>,
    pub build_log_wrapped_width: usize,
    pub build_log_viewport_height: usize,
    pub build_log_filled: usize,
    /// Whether docker-build capture is currently active.
    pub build_log_active: bool,
    /// Pointer hover state for clickable footer spans.
    pub footer_hover: StatusFooterHover,
    pub label_transition: Option<StageLabelTransition>,
    /// Pointer is hovering a copyable value in the failure popup.
    pub failure_copy_hover: Option<FailureCopyTarget>,
    /// Last failure-popup value copied via OSC 52. Drives visible feedback.
    pub failure_copied: Option<FailureCopyTarget>,
    /// Last failure-popup file path revealed through the host file manager.
    pub failure_revealed: Option<FailureCopyTarget>,
    /// Last failure-popup file path opened through the host file manager.
    pub failure_opened: Option<FailureCopyTarget>,
    /// Operator opened the shared container info dialog from the footer chip.
    pub container_info_open: bool,
    /// Last copied row in the container info dialog.
    pub container_info_copied: Option<usize>,
    /// Row in the container info dialog the pointer is hovering (a copyable
    /// value), driving the link hover-colour change.
    pub container_info_hover: Option<usize>,
    /// Scroll offsets for the container info dialog body. The state is rebuilt
    /// each frame, so the offset persists here and is threaded into the rebuilt
    /// `ContainerInfoState` — long paths scroll instead of clipping.
    pub container_info_scroll: jackin_tui::components::DialogBodyScroll,
    /// Operator pressed Ctrl+Q: the "Exit jackin❯?" confirmation overlays the
    /// cockpit and owns input until answered. `None` = not confirming. Ctrl+C
    /// bypasses this entirely (immediate hard cancel).
    pub quit_confirm: Option<jackin_tui::components::ConfirmState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCopyTarget {
    RunId,
    DiagnosticsPath,
    CommandOutputPath,
}

#[derive(Debug, Clone, Copy)]
pub struct StageLabelTransition {
    pub from: usize,
    pub to: usize,
    pub start_frame: usize,
}

// Re-exported from `jackin_core` (Workstream 1 — architecture/boundaries:
// the type was here because `jackin_launch` was a TUI, not the launch
// orchestrator. Lower crates (`jackin_env::env_resolver`) needed
// `PromptResult` and had to depend upward on this crate purely for the
// type. Both directions now point inward through `jackin_core`.)
pub use jackin_core::PromptResult;

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
