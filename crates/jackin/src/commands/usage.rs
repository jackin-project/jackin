// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use jackin_core::{HostPath, Timestamp};
use jackin_usage::{
    AccountUsage, Freshness, HostUsage, LimitWindow, Refresh, UsageAccount, UsageDeps,
    UsageOutcome, UsageReport, UsageRequest,
};

use crate::app::App;
use crate::cli::JsonArg;
use crate::error::CommandError;

pub(crate) const STATE_DIR: &str = "usage";

pub(crate) async fn run(app: &mut App, arg: &JsonArg) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let host = HostUsage::new(UsageDeps {
        clock: Arc::clone(&app.deps.host.clock),
        fs: Arc::clone(&app.deps.host.fs),
        runner: Arc::clone(&app.deps.host.runner),
        env: app.env.clone(),
        endpoints: app.deps.usage_endpoints.clone(),
        state_dir: HostPath::resolve(Path::new(STATE_DIR), &app.cache_dir)?,
    });
    let request = UsageRequest {
        accounts: config.accounts().map(UsageAccount::from_account).collect(),
        refresh: Refresh::Cached,
    };
    let report = host.report(&request, &app.cancel).await?;
    if arg.json {
        return app.io.json(&report);
    }
    for line in lines(&report) {
        app.io.line(&line)?;
    }
    Ok(())
}

fn lines(report: &UsageReport) -> Vec<String> {
    report
        .accounts
        .iter()
        .flat_map(|usage| account_lines(usage, report.generated_at))
        .collect()
}

fn account_lines(usage: &AccountUsage, now: Timestamp) -> Vec<String> {
    let head = format!("{}\t{}", usage.account, usage.provider.as_str());
    match &usage.outcome {
        UsageOutcome::Unavailable(reason) => {
            vec![format!("{head}\tunavailable\t{reason}")]
        }
        UsageOutcome::Windows(windows) => windows
            .iter()
            .map(|window| {
                let mut line = format!("{head}\t{}", window_text(window, now));
                if let Freshness::Stale { since } = usage.freshness {
                    line.push_str(&format!("\tstale, fetched {}", since.rfc3339()));
                }
                line
            })
            .collect(),
    }
}

fn window_text(window: &LimitWindow, now: Timestamp) -> String {
    format!(
        "{}\t{}\t{}\t{}",
        window.label,
        window.used_text(),
        window.remaining_text(),
        window.reset_text(now)
    )
}
