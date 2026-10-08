// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `cleanup`.

use super::super::coordination;
use super::super::naming::matching_family;

use super::*;

use crate::instance::{DockerResources, InstanceManifest};

use crate::runtime::launch::LoadCleanup;

use jackin_core::RoleSelector;

use jackin_core::{DockerApi, JackinPaths};

use jackin_docker::docker_client::{ContainerRow, ContainerState, NetworkRow};

use jackin_runtime_cleanup_prune_dir::prune_dir::prune_dir;

use jackin_test_support::{FakeDockerClient, FakeRunner};

use std::collections::{HashMap, VecDeque};

use tempfile::TempDir;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
