// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch failure rendering and the exit outro.
//!
//! The failure title/diagnosis/CLI-error rendering plus role-source
//! resolution moved to [`jackin_runtime_launch_failure::failure`] (S7
//! split 90); the item re-exports keep every
//! `jackin_runtime::runtime::launch::failure::*` path stable for existing
//! callers. `render_exit` stays here: it observes the universe exit
//! boundary (`jackin-runtime-universe`, T7), which would push the leaf
//! over the T6 tier ceiling.

pub(crate) use jackin_runtime_launch_failure::failure::{
    launch_failure_cli_error, launch_failure_title, resolve_launch_role_source,
    short_launch_diagnosis,
};

use jackin_core::JackinPaths;
use jackin_diagnostics;
use jackin_docker::docker_client::DockerApi;

use crate::instance::InstanceIndex;
use crate::runtime::universe::ExitClaim;

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

    if !running.is_empty() {
        if let Some(run) = jackin_diagnostics::active_run() {
            let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap_or(InstanceIndex {
                version: 0,
                instances: Vec::new(),
            });
            let (headline, rows) = crate::runtime::exit_summary::summary(&running, &index);
            run.compact(
                "exit_summary",
                &format!("{headline}; boundary outro skipped"),
            );
            for row in rows {
                run.compact("exit_summary", &row);
            }
        }
        if !force_outro {
            return;
        }
    }

    // Last container left the construct: clear the session marker and show the
    // two-screen outro (decelerating warp, then closing caption). Exits that
    // leave other instances running skip this entirely because the operator is
    // still inside the Construct.
    let elapsed = if force_outro && !running.is_empty() {
        None
    } else {
        match exit_claim {
            ExitClaim::Claimed { elapsed } => elapsed,
            ExitClaim::Missing if force_outro => None,
            ExitClaim::Missing => return,
        }
    };
    if !crate::runtime::progress::rich_terminal_supported() {
        return;
    }
    // Defensive: the attach paths already re-assert the alt screen the moment
    // the capsule exec returns, so the post-attach work never flashes the
    // shell. Re-assert once more before the rich outro in case render_exit is
    // reached by a path that did not go through the attach.
    jackin_diagnostics::reassert_alt_screen();
    let host_owned = jackin_diagnostics::host_screen_owned();
    crate::runtime::progress::launch_output().warp_out(host_owned);
    crate::runtime::progress::launch_output().warp_end_caption(elapsed, host_owned);
}
