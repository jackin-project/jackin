// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `caffeinate`.

use std::cell::RefCell;

use std::collections::{HashMap, VecDeque};

use super::*;

use jackin_core::ContainerRow;

use jackin_test_support::{FakeDockerClient, FakeRunner};

use tempfile::tempdir;

mod case_01;
