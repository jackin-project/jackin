// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `view`.

use super::*;

use crate::tui::screens::workspaces::model::ManagerListRow;

use jackin_core::InstanceStatus;

use ratatui::{Terminal, backend::TestBackend, layout::Rect};

mod support;
use support::*;
mod case_01;
mod case_02;
