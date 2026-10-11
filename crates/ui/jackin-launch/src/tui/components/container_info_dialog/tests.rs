// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::layout::Rect;

use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

use crate::tui::model::{LaunchIdentity, LaunchTargetKind, LaunchView};

use crate::tui::update::initial_view;

use crate::tui::view::render_launch_frame;

use super::{launch_container_info_rect, launch_container_info_state};

mod support;
use support::*;
mod case_01;
