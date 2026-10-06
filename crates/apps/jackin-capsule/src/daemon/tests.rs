// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for `jackin-capsule` daemon: input dispatch, session management,
//! tab lifecycle, git context, status-bar rendering, and PTY session behavior.

use super::*;

use crate::attach_protocol::initial_spawn_request;

use std::collections::BTreeMap;

use crate::tui::socket_backend::SgrMetadata;

use std::io;

use std::sync::{Arc, Mutex};

use termpane::DamageGrid;

use crate::pr_context::{command_output_or_lookup_error, command_stdout_trimmed};

use crate::protocol::attach::read_server_frame;

use crate::tui::components::dialog::PullRequestStatus;

use portable_pty::{ChildKiller, MasterPty, PtySize};

use tokio::io::AsyncReadExt;

use crate::tui::model::{CursorVisibilityState, cursor_visible_for_state};

use termpane::Cell;

mod support_01;
pub(crate) use support_01::*;
mod support_02;
mod support_02_fixtures;
mod support_02_virtual_client;
use support_02::*;
mod support_03;
use support_03::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
mod case_06;
mod case_07;
mod case_08;
mod case_09;
mod case_10;
mod case_11;
mod case_12;
mod case_13;
mod case_14;
mod case_15;
mod case_16;
mod case_17;
mod case_18;
mod case_19;
mod case_20;
mod case_21;
mod case_22;
mod case_23;
mod case_24;
mod case_25;
