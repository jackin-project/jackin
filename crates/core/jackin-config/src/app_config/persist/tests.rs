// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `persist`.

use super::*;

use crate::persist::{
    StagedDelete, acquire_config_write_lock, atomic_write, leak_staged_writes,
    publication_journal_path, stage_atomic_write, write_publication_journal,
};

use crate::{CURRENT_CONFIG_VERSION, CURRENT_WORKSPACE_VERSION};

use jackin_core::JackinPaths;

use std::path::Path;

use std::sync::mpsc;

use std::time::Duration as TestDuration;

use tempfile::tempdir;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
