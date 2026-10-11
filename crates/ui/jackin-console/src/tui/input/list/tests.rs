// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `list`.
//! List-stage tests: row-0 (current dir) gating, Enter routing,
//! `o`-key resolver to GitHub URLs, and the `GithubPicker` modal.

use super::super::InputOutcome;

use super::*;

use crate::tui::input::fixtures::{key, mount};

use crate::tui::message::ConsoleInstanceAction;

use crate::tui::state::AgentChoiceState;

use crate::tui::state::{ManagerStage, ManagerState, Modal, MountScrollFocus};

use crossterm::event::{KeyCode, KeyEvent};

use jackin_config::AppConfig;

use jackin_config::WorkspaceConfig;

use jackin_core::JackinPaths;

use jackin_core::{InstanceIndexEntry, InstanceStatus};

use ratatui::layout::Rect;

use tempfile::TempDir;

use support::ManagerEffect;
mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
