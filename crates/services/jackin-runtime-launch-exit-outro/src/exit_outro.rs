// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_diagnostics;
use jackin_instance::InstanceIndex;
use jackin_runtime_exit_summary::exit_summary::summary;
use jackin_runtime_progress::progress::{launch_output, rich_terminal_supported};
use jackin_runtime_universe_claims::claims::ExitClaim;

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

    jackin_diagnostics::reassert_alt_screen();
    let host_owned = jackin_diagnostics::host_screen_owned();
    launch_output().warp_out(host_owned);
    launch_output().warp_end_caption(elapsed, host_owned);
}
