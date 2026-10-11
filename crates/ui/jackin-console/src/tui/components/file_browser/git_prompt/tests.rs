// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `git_prompt`.

use super::*;

use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use std::path::Path;

use tempfile::tempdir;

mod support;
use support::*;
mod case_01;
