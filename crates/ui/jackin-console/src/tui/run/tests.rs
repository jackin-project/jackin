// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `run`.

use super::*;

use crate::tui::model::ConsoleManagerStageRoute;

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseEvent, MouseEventKind,
};

use ratatui::layout::Rect;

mod support;
use support::*;
mod case_01;
mod case_02;
