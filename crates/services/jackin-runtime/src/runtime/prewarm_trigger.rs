// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Background image-prewarm trigger (D22).
//!
//! Moved to [`jackin_runtime_prewarm_trigger::prewarm_trigger`]; this module
//! keeps the `jackin_runtime::runtime::prewarm_trigger::*` paths stable for
//! existing callers. Unit coverage stays in the hub suite below (it names
//! leaf items plus hub-only scope restoration).
//!
//! The two `spawn_*` entry points keep test-only wrappers here: their
//! `#[cfg(test)]` disabled-in-unit-tests branches keyed off the hub crate's
//! own test build, which the leaf no longer observes. Production builds
//! re-export the leaf implementations unchanged.

pub use jackin_runtime_prewarm_trigger::prewarm_trigger::{
    BackgroundPrewarmTarget, SidecarPrewarmOutcome, background_prewarm_targets,
    classify_sidecar_prewarm_attempt,
};

#[cfg(not(test))]
pub use jackin_runtime_prewarm_trigger::prewarm_trigger::{
    spawn_background_image_prewarm, spawn_background_sidecar_prewarm,
};

// Test-only scope restoration: the hub suite below was written against
// `use super::*` when this module owned the items plus these imports.
#[cfg(test)]
use jackin_config::AppConfig;
#[cfg(test)]
use jackin_core::{Agent, JackinPaths};

/// Test-only wrapper: background image prewarm stays disabled in unit tests.
///
/// Same body as the pre-split `#[cfg(test)]` branch; production builds use
/// the leaf implementation.
#[cfg(test)]
pub fn spawn_background_image_prewarm(
    paths: &JackinPaths,
    targets: Vec<BackgroundPrewarmTarget>,
    debug: bool,
) {
    if targets.is_empty() {
        return;
    }
    let _ = (paths, debug);
    if let Some(run) = jackin_diagnostics::active_run() {
        run.stage(
            "background_image_prewarm_skipped",
            jackin_diagnostics::DiagnosticStage::DerivedImage,
            "background image prewarm disabled in unit tests",
            Some(&targets.len().to_string()),
        );
    }
}

/// Test-only wrapper: background sidecar prewarm stays disabled in unit tests.
///
/// Same body as the pre-split `#[cfg(test)]` branch; production builds use
/// the leaf implementation.
#[cfg(test)]
pub fn spawn_background_sidecar_prewarm(paths: &JackinPaths, debug: bool) {
    let _ = (paths, debug);
    if let Some(run) = jackin_diagnostics::active_run() {
        run.stage(
            "background_sidecar_prewarm_skipped",
            jackin_diagnostics::DiagnosticStage::Sidecar,
            "background sidecar prewarm disabled in unit tests",
            None,
        );
    }
}

#[cfg(test)]
mod tests;
