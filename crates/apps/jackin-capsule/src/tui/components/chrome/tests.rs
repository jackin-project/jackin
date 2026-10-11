// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `chrome`.

use super::*;

use crate::tui::components::status_bar::status_bar_plan;

use crate::tui::layout::Tab;

use crate::tui::model::VisibleAgentState;

use ratatui::{Terminal, backend::TestBackend};

use termrock::style::DesignSystem;

mod support;
use support::*;
mod case_01;
