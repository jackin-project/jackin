// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `role_picker`.

use super::*;

use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

mod support;
use support::*;
mod case_01;
