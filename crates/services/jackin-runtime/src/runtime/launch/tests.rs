// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `runtime/launch.rs`: load pipeline behavioral verification.
#![expect(
    unused_qualifications,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "isolated filesystem and child-process fixtures run only on test threads"
)]

use super::super::universe;
use super::mounts::AppleContainerMountError;

use super::*;

use crate::runtime::launch::launch_runtime::{
    CapsuleAuth, CapsuleEndpoint, CapsuleNetwork, capsule_export_coverage,
    capsule_otlp_allowlist_host, capsule_otlp_propagation, debug_runtime_envs,
    telemetry_runtime_envs_for,
};

use std::path::PathBuf;

use jackin_config::AppConfig;

use jackin_core::WorkspaceName;

use jackin_test_support::FakeRunner;

use std::collections::HashMap;

use crate::isolation::MountIsolation;

use crate::isolation::materialize::{MaterializedMount, MaterializedWorkspace, WorktreeAuxMounts};

use jackin_core::JackinPaths;

use jackin_core::RoleSelector;

use std::collections::VecDeque;

use std::sync::Arc;

use std::sync::atomic::{AtomicUsize, Ordering};

use tempfile::tempdir;

mod support_01;
use support_01::*;
mod support_02;
use support_02::*;
mod support_03;
use support_03::*;
mod support_04;
use support_04::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
mod case_06;
mod case_07;
mod case_08;
mod case_09;
mod case_10;
mod case_11;
mod case_12;
mod case_13;
mod case_14;
mod case_15;
mod case_16;
mod case_17;
mod case_18;
mod case_19;
mod case_20;
mod case_21;
mod case_22;
mod case_23;
mod case_24;
mod case_25;
mod case_26;
mod case_27;
mod case_28;
mod case_29;
