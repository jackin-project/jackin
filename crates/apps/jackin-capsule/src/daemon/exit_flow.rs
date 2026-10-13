// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Last-session exit flow and dirty-exit modal entry.

use std::sync::Arc;

use crate::attach_protocol::drain_and_exit_with_reason;

use crate::tui::components::dialog::{Dialog, InspectRow};

use crate::tui::update::FullRedrawReason;

use super::Multiplexer;

/// Build the read-only Inspect rows for the dirty-exit modal: a section header
/// per dirty repo followed by its `<status> <path>` change rows.
pub(crate) fn build_exit_inspect_rows(
    repos: &[crate::exit_assess::DirtyRepo],
) -> Arc<[InspectRow]> {
    use crate::tui::components::dialog::InspectRow;
    let mut rows = Vec::new();
    for repo in repos {
        rows.push(InspectRow::Repo(repo.label().to_owned()));
        for f in &repo.changed {
            rows.push(InspectRow::File(format!("{} {}", f.status, f.path)));
        }
    }
    rows.into()
}

/// Handle the last live session exiting. Returns `true` when the daemon should
/// exit and `false` to keep the event loop running — either because a dirty-exit
/// modal was just opened, or because the modal flow is already in progress
/// (re-entry guard). With policy `ask` and dirty isolated work the modal is
/// shown (no teardown); otherwise the container drains and exits, preserving the
/// original non-clean-exit reason.
pub(crate) async fn handle_last_session_exit(
    mux: &mut Multiplexer,
    reason: Option<String>,
) -> bool {
    // Called from two sites: the session-exit event handler (once, on last-session
    // exit) and the client-frame handler (on every frame while no sessions remain).
    // The guard below handles the client-frame re-entry case: if a dialog is already
    // open (modal, Inspect view, or New-tab picker launched from "Start a new agent")
    // the dirty-exit flow is already active. Re-entering would push a fresh modal
    // and re-run the git assessment on every keypress, resetting selection to 0 —
    // so the operator could never move past the first row. Defer until resolved.
    use super::ports::{ExitDisposition, PORTS, PersistencePort};
    if PORTS.last_session_exit(&mux.control) == ExitDisposition::Defer {
        return false;
    }
    match crate::exit_assess::decide_exit(mux.launch_env.config()).await {
        crate::exit_assess::ExitDecision::Drain => {
            drain_and_exit_with_reason(mux, reason).await;
            true
        }
        crate::exit_assess::ExitDecision::DrainWithAction(action) => {
            // Policy keep/discard: record the action for the host, no prompt.
            // Write failure is logged but does not block exit — a configured
            // policy path cannot stall indefinitely waiting for a broken fs.
            if let Err(error) = crate::exit_assess::write_exit_action(action) {
                let _warning = jackin_telemetry::record_recovered_degradation();
                crate::output::stderr_line(format_args!(
                    "[daemon] exit: failed to write exit-action file, policy will not be applied: {error}"
                ));
            }
            drain_and_exit_with_reason(mux, reason).await;
            true
        }
        crate::exit_assess::ExitDecision::ShowModal(repos) => {
            let summary = repos
                .iter()
                .map(crate::exit_assess::DirtyRepo::summary_line)
                .collect();
            let inspect_rows = build_exit_inspect_rows(&repos);
            mux.dialog_push(Dialog::new_exit_dirty(summary, inspect_rows));
            mux.invalidate(FullRedrawReason::DialogChange);
            false
        }
    }
}
