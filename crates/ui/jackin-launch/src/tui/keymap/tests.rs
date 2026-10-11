// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::build_log_hint_spans;
use super::cockpit_global_hint_spans;
use termrock::input::KeyCode;

use termrock::keymap::{KeyChord, glyph};

use super::{
    BUILD_LOG_KEYMAP, BuildLogAction, COCKPIT_KEYMAP, CONTAINER_INFO_KEYMAP, CockpitAction,
    ContainerInfoAction, FAILURE_KEYMAP, FailureAction,
};

mod support;
use support::*;
mod case_01;
