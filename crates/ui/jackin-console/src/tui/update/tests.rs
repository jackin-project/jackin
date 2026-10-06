// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `update`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};

use super::*;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
