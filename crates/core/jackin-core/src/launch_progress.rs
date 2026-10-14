// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::path::Path;

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

#[derive(Debug, Clone)]
pub struct LaunchIdentity {
    pub role: String,
    pub agent: String,
    pub target_kind: LaunchTargetKind,
    pub target_label: String,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptContextLine {
    Emphasis(String),
    Muted(String),
    Path(String),
    Plain(String),
    Blank,
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub status: char,
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorktreeInspect {
    pub label: String,
    pub files: Vec<FileDiff>,
}

#[derive(Debug, Clone)]
pub struct LaunchCandidate {
    pub label: String,
    pub is_dirty: bool,
    pub inspect: Vec<WorktreeInspect>,
}

#[derive(Debug, Clone)]
pub enum LaunchDialogResult {
    StartFresh,
    Restore(usize),
    Delete(usize),
}

#[derive(Debug)]
pub struct LaunchCancelled;

impl std::fmt::Display for LaunchCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("launch cancelled by operator")
    }
}

impl std::error::Error for LaunchCancelled {}

impl LaunchCancelled {
    pub fn err() -> anyhow::Error {
        anyhow::Error::new(Self)
    }

    pub fn is_cancel(error: &anyhow::Error) -> bool {
        error.downcast_ref::<Self>().is_some()
    }
}

pub trait LaunchDiagnostics: Send + Sync {
    fn run_id(&self) -> &str;

    fn compact(&self, kind: &str, message: &str);
    fn error(&self, kind: &str, message: &str, error_type: Option<&str>);
    fn stage(&self, kind: &str, stage: LaunchStage, message: &str, detail: Option<&str>);
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

pub trait LaunchOutputSink: Send + Sync {
    fn print_deploying<'a>(
        &'a self,
        role_name: &'a str,
    ) -> std::pin::Pin<Box<dyn Future<Output = ()> + 'a>>;
    fn step_fail(&self, msg: &str);
    fn warp_out(&self, host_screen_owned: bool);
    fn warp_end_caption(&self, elapsed: Option<std::time::Duration>, host_screen_owned: bool);
}
