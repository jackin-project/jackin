// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `runtime_setup`.

use super::*;

use std::fs;

use std::path::Path;

use std::sync::{
    Arc, Barrier,
    atomic::{AtomicBool, Ordering},
};

mod case_01;
mod case_02;
