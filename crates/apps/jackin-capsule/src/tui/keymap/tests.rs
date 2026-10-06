// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    CAPSULE_GLOBAL_KEYMAP, FILTER_LIST_KEYMAP, FilterListAction, GlobalCapsuleAction,
    PREFIX_COMMAND_KEYMAP, READ_ONLY_DISMISS_KEYMAP, RENAME_KEYMAP, RESIZE_PANE_KEYMAP,
    ReadOnlyDismissAction, RenameAction,
};

use crate::tui::input::{ArrowDir, PrefixCommand};

use crate::tui::keymap::raw_bytes_to_chord;

use termrock::input::{KeyChord, KeyCode};

mod support;
use support::*;
mod case_01;
