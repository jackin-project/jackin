// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `image`.

use super::*;

use jackin_core::Agent;

#[cfg(unix)]
use jackin_core::{CommandRunner, RunOptions};

use jackin_image::{
    LABEL_IMAGE_CAPSULE_VERSION, LABEL_IMAGE_MANIFEST_VERSION, LABEL_IMAGE_RECIPE_HASH,
    LABEL_IMAGE_RECIPE_VERSION, image_recipe::build_image_recipe,
};

use jackin_test_support::{FakeDockerClient, FakeRunner};

use std::collections::{BTreeMap, HashMap};

use std::sync::{Mutex, MutexGuard};

#[cfg(unix)]
use std::{os::unix::fs::PermissionsExt, path::Path, process::Command as ProcessCommand};

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
