// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
pub use session_cmds::{
    run_session_events, run_session_release_control, run_session_send, run_session_take_control,
};
pub use status::{
    run_snapshot, run_status, run_status_capture, run_status_explain, run_token_usage,
};
pub use usage::{run_usage_accounts, run_usage_verify};

pub(crate) use control::{
    connect_and_send, read_control_reply, read_control_reply_or_eof, request_control,
};

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}
