// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `pane`.

use super::*;

use crate::tui::socket_backend::term_color;

use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::{Buffer, CellDiffOption},
    layout::Rect,
    style::{Color, Modifier},
};

use std::num::NonZeroU16;

use termpane::DamageGrid;

mod case_01;
