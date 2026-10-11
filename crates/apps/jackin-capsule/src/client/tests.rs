// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `client`.

use super::*;

use std::path::PathBuf;

use tempfile::TempDir;

use tokio::net::UnixListener;

mod support;
use support::*;
mod case_01;
