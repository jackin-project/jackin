// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::attach_protocol::drain_and_exit_with_reason;

use crate::tui::components::dialog::{Dialog, InspectRow};

use crate::tui::update::FullRedrawReason;

use super::Multiplexer;

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

pub(crate) async fn handle_last_session_exit(
    mux: &mut Multiplexer,
    reason: Option<String>,
) -> bool {
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
