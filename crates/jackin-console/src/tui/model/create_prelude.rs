// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `ConsoleCreatePreludeState` and the `CreatePrelude*` plan types and free functions.
use super::stage::ConsoleCreatePreludeModalPresence;
use crate::tui::debug::{ConsoleCreatePreludeDebugFacts, ConsoleModalDebugKind, ConsoleStageDebug};
use crate::tui::state::CreateStep;

use std::path::PathBuf;

#[derive(Debug)]
pub struct ConsoleCreatePreludeState<Modal> {
    pub step: CreateStep,
    pub pending_mount_src: Option<PathBuf>,
    pub pending_mount_dst: Option<String>,
    pub pending_readonly: bool,
    pub pending_workdir: Option<String>,
    pub pending_name: Option<String>,
    pub modal: Option<Modal>,
    /// Captured so Esc on `MountDstChoice` re-opens `FileBrowser` at the same
    /// directory instead of `$HOME`.
    pub last_browser_cwd: Option<PathBuf>,
    /// Picks Esc-on-`WorkdirPick` rewind target: `TextInputDst` when the
    /// Edit-destination branch was used, else `MountDstChoice`.
    pub used_edit_dst: bool,
}

impl<Modal> ConsoleCreatePreludeModalPresence for ConsoleCreatePreludeState<Modal> {
    fn create_prelude_modal_open(&self) -> bool {
        self.modal.is_some()
    }
}

impl<Modal> ConsoleCreatePreludeDebugFacts for ConsoleCreatePreludeState<Modal>
where
    Modal: ConsoleModalDebugKind,
{
    fn create_prelude_stage_debug(&self) -> ConsoleStageDebug {
        ConsoleStageDebug::CreatePrelude {
            step: format!("{:?}", self.step),
            modal: self
                .modal
                .as_ref()
                .map(ConsoleModalDebugKind::modal_debug_kind),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatePreludeCompletionStatus {
    InProgress,
    Complete,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatePreludeKeyPlan {
    Continue,
    ReturnToList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatePreludeModalStep {
    FileBrowserSrc,
    MountDstChoice,
    TextInputDst,
    WorkdirPick,
    TextInputName,
    Other,
}

pub trait CreatePreludeFileBrowserTarget {
    fn is_create_first_mount_src(&self) -> bool;
}

pub trait CreatePreludeTextInputTarget {
    fn is_create_mount_dst(&self) -> bool;
    fn is_create_workspace_name(&self) -> bool;
}

impl CreatePreludeFileBrowserTarget for crate::tui::screens::editor::model::FileBrowserTarget {
    fn is_create_first_mount_src(&self) -> bool {
        matches!(self, Self::CreateFirstMountSrc)
    }
}

impl CreatePreludeTextInputTarget for crate::tui::screens::editor::model::TextInputTarget {
    fn is_create_mount_dst(&self) -> bool {
        matches!(self, Self::MountDst)
    }

    fn is_create_workspace_name(&self) -> bool {
        matches!(self, Self::Name)
    }
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Five orthogonal modal-input availability booleans (file_browser_src, \
              mount_dst_choice, text_input_dst, workdir_pick, text_input_name) — \
              each is an independent input-mode signal the step resolver reads in \
              priority order. Named-arg reads match the per-mode priority-routing \
              idiom this resolver walks."
)]
#[must_use]
pub const fn create_prelude_modal_step(
    file_browser_src: bool,
    mount_dst_choice: bool,
    text_input_dst: bool,
    workdir_pick: bool,
    text_input_name: bool,
) -> CreatePreludeModalStep {
    if file_browser_src {
        CreatePreludeModalStep::FileBrowserSrc
    } else if mount_dst_choice {
        CreatePreludeModalStep::MountDstChoice
    } else if text_input_dst {
        CreatePreludeModalStep::TextInputDst
    } else if workdir_pick {
        CreatePreludeModalStep::WorkdirPick
    } else if text_input_name {
        CreatePreludeModalStep::TextInputName
    } else {
        CreatePreludeModalStep::Other
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatePreludeFileBrowserPlan<T> {
    CancelPrelude,
    ResolveGitUrl(PathBuf),
    OpenUrl(String),
    ApplyFileBrowserOutcome(crate::tui::components::file_browser::FileBrowserOutcome<T>),
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatePreludeWorkdirCancelPlan {
    ReopenTextInputDst,
    ReopenMountDstChoice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatePreludeWorkdirPickPlan<T> {
    Commit(T),
    ReopenTextInputDst,
    ReopenMountDstChoice,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatePreludeMountDstChoicePlan {
    CommitSamePath,
    OpenEditInput,
    ReopenFileBrowserAtLastCwd,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatePreludeTextInputDstPlan<T> {
    Commit(T),
    ReopenMountDstChoice,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatePreludeTextInputNamePlan<T> {
    Commit(T),
    ReopenWorkdirPick,
    Continue,
}

#[must_use]
pub fn create_prelude_text_input_dst_plan<T>(
    outcome: jackin_tui::ModalOutcome<T>,
) -> CreatePreludeTextInputDstPlan<T> {
    match outcome {
        jackin_tui::ModalOutcome::Commit(dst) => CreatePreludeTextInputDstPlan::Commit(dst),
        jackin_tui::ModalOutcome::Cancel => CreatePreludeTextInputDstPlan::ReopenMountDstChoice,
        jackin_tui::ModalOutcome::Continue => CreatePreludeTextInputDstPlan::Continue,
    }
}

#[must_use]
pub fn create_prelude_text_input_name_plan<T>(
    outcome: jackin_tui::ModalOutcome<T>,
) -> CreatePreludeTextInputNamePlan<T> {
    match outcome {
        jackin_tui::ModalOutcome::Commit(name) => CreatePreludeTextInputNamePlan::Commit(name),
        jackin_tui::ModalOutcome::Cancel => CreatePreludeTextInputNamePlan::ReopenWorkdirPick,
        jackin_tui::ModalOutcome::Continue => CreatePreludeTextInputNamePlan::Continue,
    }
}

#[must_use]
pub fn create_prelude_workdir_pick_plan<T>(
    outcome: jackin_tui::ModalOutcome<T>,
    used_edit_dst: bool,
) -> CreatePreludeWorkdirPickPlan<T> {
    match outcome {
        jackin_tui::ModalOutcome::Commit(workdir) => CreatePreludeWorkdirPickPlan::Commit(workdir),
        jackin_tui::ModalOutcome::Cancel => match create_prelude_workdir_cancel_plan(used_edit_dst)
        {
            CreatePreludeWorkdirCancelPlan::ReopenTextInputDst => {
                CreatePreludeWorkdirPickPlan::ReopenTextInputDst
            }
            CreatePreludeWorkdirCancelPlan::ReopenMountDstChoice => {
                CreatePreludeWorkdirPickPlan::ReopenMountDstChoice
            }
        },
        jackin_tui::ModalOutcome::Continue => CreatePreludeWorkdirPickPlan::Continue,
    }
}

#[must_use]
pub fn create_prelude_file_browser_plan<T>(
    outcome: crate::tui::components::file_browser::FileBrowserOutcome<T>,
) -> CreatePreludeFileBrowserPlan<T> {
    match outcome {
        crate::tui::components::file_browser::FileBrowserOutcome::Cancel => {
            CreatePreludeFileBrowserPlan::CancelPrelude
        }
        crate::tui::components::file_browser::FileBrowserOutcome::ResolveGitUrl(path) => {
            CreatePreludeFileBrowserPlan::ResolveGitUrl(path)
        }
        crate::tui::components::file_browser::FileBrowserOutcome::OpenGitUrl(url) => {
            CreatePreludeFileBrowserPlan::OpenUrl(url)
        }
        crate::tui::components::file_browser::FileBrowserOutcome::Continue => {
            CreatePreludeFileBrowserPlan::Continue
        }
        crate::tui::components::file_browser::FileBrowserOutcome::Commit(_)
        | crate::tui::components::file_browser::FileBrowserOutcome::NavigateTo(_)
        | crate::tui::components::file_browser::FileBrowserOutcome::NavigateUp
        | crate::tui::components::file_browser::FileBrowserOutcome::RequestCommit(_) => {
            CreatePreludeFileBrowserPlan::ApplyFileBrowserOutcome(outcome)
        }
    }
}

#[must_use]
pub const fn create_prelude_mount_dst_choice_plan(
    outcome: jackin_tui::ModalOutcome<crate::tui::components::mount_dst_choice::MountDstChoice>,
) -> CreatePreludeMountDstChoicePlan {
    match outcome {
        jackin_tui::ModalOutcome::Commit(
            crate::tui::components::mount_dst_choice::MountDstChoice::SamePath,
        ) => CreatePreludeMountDstChoicePlan::CommitSamePath,
        jackin_tui::ModalOutcome::Commit(
            crate::tui::components::mount_dst_choice::MountDstChoice::Edit,
        ) => CreatePreludeMountDstChoicePlan::OpenEditInput,
        jackin_tui::ModalOutcome::Cancel => {
            CreatePreludeMountDstChoicePlan::ReopenFileBrowserAtLastCwd
        }
        jackin_tui::ModalOutcome::Continue => CreatePreludeMountDstChoicePlan::Continue,
    }
}

#[must_use]
pub const fn create_prelude_workdir_cancel_plan(
    used_edit_dst: bool,
) -> CreatePreludeWorkdirCancelPlan {
    if used_edit_dst {
        CreatePreludeWorkdirCancelPlan::ReopenTextInputDst
    } else {
        CreatePreludeWorkdirCancelPlan::ReopenMountDstChoice
    }
}

#[must_use]
pub const fn create_prelude_key_plan(key: crossterm::event::KeyCode) -> CreatePreludeKeyPlan {
    match key {
        crossterm::event::KeyCode::Esc => CreatePreludeKeyPlan::ReturnToList,
        _ => CreatePreludeKeyPlan::Continue,
    }
}

#[must_use]
pub const fn create_prelude_completion_status(
    modal_open: bool,
    completed: bool,
) -> CreatePreludeCompletionStatus {
    if modal_open {
        CreatePreludeCompletionStatus::InProgress
    } else if completed {
        CreatePreludeCompletionStatus::Complete
    } else {
        CreatePreludeCompletionStatus::Cancelled
    }
}

impl<Modal> Default for ConsoleCreatePreludeState<Modal> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Modal> ConsoleCreatePreludeState<Modal> {
    #[must_use]
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

    pub fn accept_mount_src(&mut self, src: PathBuf) {
        self.pending_mount_src = Some(src);
        self.step = CreateStep::PickFirstMountDst;
    }

    /// Default mount dst = same absolute path as host src. Operator can
    /// overwrite in the dst modal.
    #[must_use]
    pub fn default_mount_dst(&self) -> String {
        let src_display = self
            .pending_mount_src
            .as_ref()
            .map(|path| path.display().to_string());
        crate::tui::screens::workspaces::view::create_prelude_mount_destination_default(
            src_display.as_deref(),
        )
    }

    pub fn accept_mount_dst(&mut self, dst: String, readonly: bool) {
        self.pending_mount_dst = Some(dst);
        self.pending_readonly = readonly;
        self.step = CreateStep::PickWorkdir;
    }

    pub fn accept_workdir(&mut self, workdir: String) {
        self.pending_workdir = Some(workdir);
        self.step = CreateStep::NameWorkspace;
    }

    /// Default name = mount dst basename.
    #[must_use]
    pub fn default_name(&self) -> String {
        crate::tui::screens::workspaces::view::create_prelude_workspace_name_default(
            self.pending_mount_dst.as_deref(),
        )
    }

    pub fn accept_name(&mut self, name: String) {
        self.pending_name = Some(name);
    }

    pub fn open_workdir_pick_from_pending_mount(
        &mut self,
        make_modal: impl FnOnce(jackin_config::MountConfig) -> Modal,
    ) -> bool {
        let Some(mount) = self.pending_first_mount() else {
            return false;
        };
        self.modal = Some(make_modal(mount));
        true
    }

    pub fn reopen_mount_dst_choice(&mut self, make_modal: impl FnOnce(String) -> Modal) {
        let src = self.default_mount_dst();
        self.modal = Some(make_modal(src));
    }

    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.pending_name.as_deref()
    }

    #[must_use]
    pub fn pending_first_mount(&self) -> Option<jackin_config::MountConfig> {
        Some(crate::services::workspace::shared_mount_config(
            self.pending_mount_src.as_ref()?.display().to_string(),
            self.pending_mount_dst.clone()?,
            self.pending_readonly,
        ))
    }

    #[must_use]
    pub fn build_workspace(&self) -> Option<jackin_config::WorkspaceConfig> {
        let workdir = self.pending_workdir.as_ref()?;

        Some(jackin_config::WorkspaceConfig {
            workdir: workdir.clone(),
            mounts: vec![self.pending_first_mount()?],
            ..jackin_config::WorkspaceConfig::default()
        })
    }

    #[must_use]
    pub fn completed(&self) -> Option<(String, jackin_config::WorkspaceConfig)> {
        let name = self.pending_name.clone()?;
        let workspace = self.build_workspace()?;
        Some((name, workspace))
    }
}
