// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Phase chain body for `run_launch_core` (typed `#[must_use]` handoffs).

mod helpers;

mod runtime_dispatch;

mod active;
mod cleanup_classify;
mod environment_prepare;
mod environment_resolve;
mod finalize;
mod finish;
mod image;
mod initialize;
mod phases;
mod prepare_instance;
mod runtime;
mod workspace;

use crate::runtime::launch::launch_pipeline::launch_phases::RuntimeLaunched;

pub(crate) use active::run_active_launch;
pub(crate) use cleanup_classify::{ClassifyCleanup, classify_cleanup};
pub(crate) use environment_prepare::prepare_environment;
pub(crate) use environment_resolve::{
    PrepareEnvironment, ResolveEnvironment, prewarm_sibling_auth_before_admission,
    resolve_environment,
};
pub(crate) use finalize::{
    FinalizeSession, finalize_session, handle_launch_failure, poll_sidecar_while,
};
pub(crate) use finish::{ActiveLaunch, FinishLaunch, finish_launch};
pub(crate) use image::{MaterializeImage, materialize_image_phase};
pub(crate) use initialize::{InitializeLaunch, LaunchInitialized, initialize_launch};
pub(crate) use phases::run_launch_phases;
pub(crate) use prepare_instance::{PrepareInstance, prepare_instance};
pub(crate) use runtime::{LaunchRuntime, launch_runtime};
pub(crate) use workspace::{MaterializeWorkspace, materialize_workspace_phase};

#[cfg(test)]
mod tests;
