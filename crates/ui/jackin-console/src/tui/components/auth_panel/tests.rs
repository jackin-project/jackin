// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `auth_panel`.

use super::*;

use jackin_core::ANTHROPIC_API_KEY_ENV_NAME;

use ratatui::{Terminal, backend::TestBackend};

mod support;
use support::*;
mod case_01;
