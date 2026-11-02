// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use jackin_core::{HostPath, Timestamp};
use jackin_usage::{
    AccountUsage, Freshness, HostUsage, LimitWindow, Refresh, Remaining, UnavailableReason,
    UsageAccount, UsageDeps, UsageOutcome, UsageReport, UsageRequest,
};

use crate::app::App;
use crate::cli::JsonArg;
use crate::error::CommandError;

const STATE_DIR: &str = "usage";
const DAY: i64 = 86_400;

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
            vec![format!("{head}\tunavailable\t{}", reason_text(reason))]
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
    let used = window.used.map_or_else(
        || "-".to_owned(),
        |used| format!("{}% used", number(used.get())),
    );
    let left = match &window.remaining {
        Some(Remaining::Percent(left)) => format!("{}% left", number(left.get())),
        Some(Remaining::Amount { value, unit }) => format!("{value} {unit} left"),
        None => "-".to_owned(),
    };
    let resets = window.resets_at.map_or_else(
        || "-".to_owned(),
        |at| format!("resets {} ({})", at.rfc3339(), distance(at.0 - now.0)),
    );
    format!("{}\t{used}\t{left}\t{resets}", window.label)
}

fn number(value: f64) -> String {
    let text = format!("{:.2}", (value * 100.0).round() / 100.0);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn distance(seconds: i64) -> String {
    if seconds <= 0 {
        "passed".to_owned()
    } else if seconds < DAY {
        format!("in {}h{}m", seconds / 3_600, seconds % 3_600 / 60)
    } else {
        format!("in {}d{}h", seconds / DAY, seconds % DAY / 3_600)
    }
}

fn reason_text(reason: &UnavailableReason) -> String {
    match reason {
        UnavailableReason::NoCredentials => "no credentials".to_owned(),
        UnavailableReason::ExpiredLogin => "login expired or rejected".to_owned(),
        UnavailableReason::Unsupported { detail: None } => {
            "not supported for this provider".to_owned()
        }
        UnavailableReason::Unsupported {
            detail: Some(detail),
        } => format!("not supported: {detail}"),
        UnavailableReason::RateLimited { retry_at: None } => "rate limited".to_owned(),
        UnavailableReason::RateLimited { retry_at: Some(at) } => {
            format!("rate limited, retry at {}", at.rfc3339())
        }
        UnavailableReason::Network(message) => format!("network error: {message}"),
        UnavailableReason::Provider(message) => format!("provider error: {message}"),
    }
}
