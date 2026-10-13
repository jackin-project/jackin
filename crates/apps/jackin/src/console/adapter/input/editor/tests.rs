// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `editor` input handlers.
//! Editor-stage tests: tab cycling, modal dispatch, role allow/default
//! bindings, and mount-row readonly toggle.

use super::super::fixtures::{key, mount};

use super::{
    EditorModalOutcome, apply_file_browser_to_editor, apply_text_input_to_pending,
    env_key_input_state, handle_editor_modal, poll_role_load, role_load_input_state,
    secret_new_key_label,
};

use crate::console::adapter::input::handle_key;

use crate::console::adapter::state::{
    AuthRow, ConfirmTarget, EditorState, EditorTab, FieldFocus, FileBrowserTarget, ManagerStage,
    ManagerState, Modal, PendingRoleLoad, SecretsRow, SecretsScopeTag, TextInputTarget,
};

use crossterm::event::KeyCode;

use jackin_config::AppConfig;

use jackin_config::{MountConfig, WorkspaceConfig};

use jackin_console::tui::auth::AuthKind;

use jackin_core::JackinPaths;

use jackin_env::OpCache;

use jackin_manifest::repo::CachedRepo;

use jackin_test_support::{FakeRunner, first_temp_role_repo, seed_valid_role_repo};

use ratatui::layout::Rect;

use tempfile::TempDir;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
mod case_06;
