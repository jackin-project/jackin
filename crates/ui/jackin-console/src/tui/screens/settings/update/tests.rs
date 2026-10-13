// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `update`.

use super::super::model;
use super::super::model::{SettingsPanelDirty, SettingsPanelDiscard};

use super::*;

use ratatui::layout::Rect;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
mod case_06;
