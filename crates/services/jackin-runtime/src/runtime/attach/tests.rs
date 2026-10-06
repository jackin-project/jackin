// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `attach`.

use super::super::backend;
use super::super::launch;
use super::super::snapshot;
use super::super::universe;
use std::collections::{HashMap, VecDeque};

use std::path::PathBuf;

use std::sync::{Mutex, OnceLock};

use super::*;

use jackin_test_support::{FakeDockerClient, FakeRunner};

use tempfile::TempDir;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
mod case_06;
