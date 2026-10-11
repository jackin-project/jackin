// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Cross-flow tests that genuinely span multiple stages. Stage-local
//! tests live in the matching `input/<stage>.rs` test module:
//! `input/list.rs`, `input/editor.rs`, `input/save.rs`,
//! `input/prelude.rs`, `input/mouse.rs`.
//!
//! Anything kept here must drive a transition that crosses two stage
//! handlers in a single test (e.g. open the in-editor rename modal,
//! commit it via `handle_key`, then drive the save flow through the
//! same `handle_key`).

use super::super::fixtures::{key, mount};

use super::*;

use crate::console::adapter::state::{
    EditorState, FieldFocus, ManagerStage, ManagerState, SettingsState,
};

use crossterm::event::KeyCode;

use jackin_config::AppConfig;

use jackin_core::JackinPaths;

mod case_01;
