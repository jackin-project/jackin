// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use crate::instance::{DockerResources, InstanceManifest};

use jackin_core::JackinPaths;

use jackin_docker::docker_client::ContainerState;

use jackin_runtime_cleanup_prune_dir::prune_dir::prune_dir;

use jackin_test_support::{FakeDockerClient, FakeRunner};

use std::collections::VecDeque;

use tempfile::TempDir;

mod support;
use support::*;
mod case_05;
