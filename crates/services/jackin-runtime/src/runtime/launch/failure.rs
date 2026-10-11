// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch failure rendering and the exit outro.
//!
//! The failure title/diagnosis/CLI-error rendering plus role-source
//! resolution moved to [`jackin_runtime_launch_failure::failure`] (S7
//! split 90); the item re-exports keep every
//! `jackin_runtime::runtime::launch::failure::*` path stable for existing
//! callers. `render_exit` keeps observing the universe exit boundary here
//! (`jackin-runtime-universe`, T7) and delegates the outro rendering to
//! [`jackin_runtime_launch_exit_outro::exit_outro`] (S7 split 95,
//! observation-inversion: the leaf renders from the already-observed
//! `(running, ExitClaim, force_outro, data_dir)` and grades T6).

pub(crate) use jackin_runtime_launch_failure::failure::{
    launch_failure_cli_error, launch_failure_title, resolve_launch_role_source,
    short_launch_diagnosis,
};

use jackin_core::JackinPaths;
use jackin_diagnostics;
use jackin_docker::docker_client::DockerApi;

pub(crate) async fn render_exit(paths: &JackinPaths, docker: &impl DockerApi) {
    let force_outro = crate::runtime::universe::force_boundary_outro_enabled();
    let (running, exit_claim) = match crate::runtime::universe::observe_exit(paths, docker).await {
        Ok(observation) => observation,
        Err(e) => {
            if let Some(run) = jackin_diagnostics::active_run() {
                run.compact(
                    "exit_summary",
                    &format!("skipping boundary outro; running-container list failed: {e:#}"),
                );
            }
            return;
        }
    };
    jackin_runtime_launch_exit_outro::exit_outro::render_exit_observation(
        &paths.data_dir,
        &running,
        exit_claim,
        force_outro,
    );
}
