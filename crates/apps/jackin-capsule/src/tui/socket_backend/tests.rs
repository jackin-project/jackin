// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `socket_backend`.

use ratatui::{
    Terminal,
    backend::{Backend, ClearType},
    layout::{Position, Rect},
    style::{Color, Modifier},
    text::Span,
    widgets::Paragraph,
};

use super::{CellStyle, SgrMetadata, SocketBackend};

mod support;
use support::*;
mod case_01;
