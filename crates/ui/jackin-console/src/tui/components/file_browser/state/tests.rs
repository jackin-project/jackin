// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `state`.

use super::*;

use crate::services::file_browser::EXCLUDED;

use ratatui::layout::Rect;

use tempfile::tempdir;

mod support;
use support::*;
mod case_01;
