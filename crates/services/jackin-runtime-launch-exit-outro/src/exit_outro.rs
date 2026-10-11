// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Exit outro rendering from an observed universe boundary.
//!
//! The hub observes the boundary (`jackin-runtime-universe`, T7) and hands
//! the result over; this module renders the outcome. Exits that leave other
//! instances running skip the outro (the operator is still inside the
//! Construct) unless the force-outro flag is set; the last container out
//! clears the session marker path and shows the two-screen outro.
//!
//! Split out of `jackin-runtime` (S7 split 95, observation-inversion); the
//! old `jackin_runtime::runtime::launch::render_exit` path keeps working
//! through the hub wrapper, which still owns the observation.

use std::path::Path;

use jackin_diagnostics;
use jackin_instance::InstanceIndex;
use jackin_runtime_exit_summary::exit_summary::summary;
use jackin_runtime_progress::progress::{launch_output, rich_terminal_supported};
use jackin_runtime_universe_claims::claims::ExitClaim;

/// Render the exit outro from an already-observed universe boundary.
///
/// `running` is the observed still-running container list, `exit_claim` the
/// claimed boundary marker, and `force_outro` the force-boundary-outro flag;
/// `data_dir` rebuilds the instance index for the still-running summary.
/// Exits that leave other instances running skip the outro unless forced.
pub fn render_exit_observation(
    data_dir: &Path,
    running: &[String],
    exit_claim: ExitClaim,
    force_outro: bool,
) {
    if !running.is_empty() {
        if let Some(run) = jackin_diagnostics::active_run() {
            let index = InstanceIndex::read_or_rebuild(data_dir).unwrap_or(InstanceIndex {
                version: 0,
                instances: Vec::new(),
            });
            let (headline, rows) = summary(running, &index);
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
    if !rich_terminal_supported() {
        return;
    }
    // Defensive: the attach paths already re-assert the alt screen the moment
    // the capsule exec returns, so the post-attach work never flashes the
    // shell. Re-assert once more before the rich outro in case render_exit is
    // reached by a path that did not go through the attach.
    jackin_diagnostics::reassert_alt_screen();
    let host_owned = jackin_diagnostics::host_screen_owned();
    launch_output().warp_out(host_owned);
    launch_output().warp_end_caption(elapsed, host_owned);
}
