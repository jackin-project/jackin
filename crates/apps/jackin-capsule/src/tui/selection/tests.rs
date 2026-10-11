// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `selection`.

use super::{
    SelectionState, move_selection_end, selection_start_for_inner_rect, selection_was_dragged,
    visible_selection,
};

use crate::tui::layout::Rect;

use crate::tui::pane_snapshot::{CellSnapshot, RowSnapshot};

use crate::tui::selection::word_bounds_in_row;

use unicode_width::UnicodeWidthChar;

mod support;
use support::*;
mod case_01;
