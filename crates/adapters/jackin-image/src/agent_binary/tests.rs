// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `agent_binary`.

use super::*;

use std::cell::Cell;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use tracing_subscriber::layer::{Context, Layer};

mod support;
use support::*;
mod case_01;
mod case_02;
