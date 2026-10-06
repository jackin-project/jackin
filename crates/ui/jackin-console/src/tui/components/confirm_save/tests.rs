// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `confirm_save`.

use super::*;

use crossterm::event::{KeyCode, KeyEventKind, KeyEventState, KeyModifiers};

use termrock::{keymap::KeyChord, widgets::HintSpan};

mod support;
use support::*;
mod case_01;
