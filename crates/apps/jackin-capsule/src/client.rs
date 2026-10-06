// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-side capsule client: connects to the daemon socket, forwards
//! stdin/stdout, and handles terminal window resize events.
//!
//! Not responsible for: daemon session management, PTY allocation, or
//! in-container rendering.

mod agents;
mod attach;
mod control;
mod report;
mod session_cmds;
mod status;
mod usage;

pub use agents::{AgentsFormat, run_agents};
pub use attach::{run_attach_proxy, run_client, run_protocol_check};
pub use report::run_report_event;
pub use session_cmds::{run_session_events, run_session_send};
pub use status::{
    run_snapshot, run_status, run_status_capture, run_status_explain, run_token_usage,
};
pub use usage::{run_usage_accounts, run_usage_verify};

#[cfg(test)]
pub(crate) use crate::protocol::control::{
    AccountUsageSnapshotView, ServerMsg, frame as control_frame,
};
#[cfg(test)]
pub(crate) use attach::run_attach_proxy_at;
pub(crate) use control::{
    connect_and_send, read_control_reply, read_control_reply_or_eof, request_control,
};
#[cfg(test)]
pub(crate) use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(test)]
pub(crate) use tokio::net::UnixStream;
#[cfg(test)]
pub(crate) use usage::verify_usage_accounts;

/// The first arg after `flag` — works for both `--flag value` and a positional
/// marker like the `<session_id>` after `explain`.
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

#[cfg(test)]
mod tests;
