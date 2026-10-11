// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `prelude`.
//! Create-wizard tests: the prelude's multi-step modal sequence
//! (`FileBrowserSrc` → `MountDstChoice` → `TextInputDst` → `WorkdirPick` →
//! `TextInputName`) and its step-back / Esc semantics.

use super::super::fixtures::key;

use super::{
    PreludeModalOutcome, create_prelude_mount_dst_choice_state, create_prelude_workdir_pick_state,
    create_prelude_workspace_name_input_state, handle_prelude_modal as raw_handle_prelude_modal,
};

use crate::tui::model::{
    CREATE_PRELUDE_STEP_MOUNT_DST_CHOICE, CREATE_PRELUDE_STEP_MOUNT_DST_EDIT,
    CREATE_PRELUDE_STEP_MOUNT_SRC, CREATE_PRELUDE_STEP_NAME, CREATE_PRELUDE_STEP_WORKDIR,
    CreatePreludeCompletionStatus, create_prelude_completion_status,
};

use crate::tui::state::{FileBrowserTarget, Modal};

use crossterm::event::KeyCode;

use ratatui::layout::Rect;

use termrock::widgets::{WizardPhase, WizardProgress};

mod support;
use support::*;
mod case_01;
mod case_02;
