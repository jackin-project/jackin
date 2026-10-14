// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub use jackin_runtime_prewarm_trigger::prewarm_trigger::{
    BackgroundPrewarmTarget, SidecarPrewarmOutcome, background_prewarm_targets,
    classify_sidecar_prewarm_attempt,
};

#[cfg(not(test))]
pub use jackin_runtime_prewarm_trigger::prewarm_trigger::{
    spawn_background_image_prewarm, spawn_background_sidecar_prewarm,
};

#[cfg(test)]
use jackin_config::AppConfig;
#[cfg(test)]
use jackin_core::{Agent, JackinPaths};

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
