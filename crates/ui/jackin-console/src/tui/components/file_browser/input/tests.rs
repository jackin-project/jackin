// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `input`.

use super::*;

use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use tempfile::tempdir;

mod support;
use support::*;
mod case_01;
mod case_02;
