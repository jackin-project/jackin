// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use anyhow::Result;
use jackin_protocol::CapsuleConfig;
use tokio::net::UnixStream;

use tokio::time::Duration;

use crate::protocol::attach::{ServerFrame, SpawnRequest, encode_server};

use crate::socket;

use crate::tui::input::{DEFAULT_ESCAPE_TIME, ENV_ESCAPE_TIME};

use super::{Multiplexer, RPC_ERROR, run_daemon_loop};

pub(crate) fn screen_detection_disabled_message(error: &anyhow::Error) -> String {
    format!("Agent status screen detection is off: {error:#}")
}

pub(crate) fn configured_escape_time() -> Duration {
    let Ok(raw) = std::env::var(ENV_ESCAPE_TIME) else {
        return DEFAULT_ESCAPE_TIME;
    };
    let Ok(ms) = raw.parse::<u64>() else {
        let _warning = jackin_telemetry::record_recovered_degradation();
        return DEFAULT_ESCAPE_TIME;
    };
    Duration::from_millis(ms)
}

pub(crate) async fn reject_invalid_attach_handshake(stream: &mut UnixStream) {
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str("jackin.capsule.Attach/Handshake"),
        },
    ];
    let operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::RPC_SERVER, &attrs).ok();
    let record = || {
        let _error = jackin_telemetry::record_error(RPC_ERROR);
    };
    if let Some(operation) = operation.as_ref() {
        operation.span().in_scope(record);
    } else {
        record();
    }
    let response = encode_server(ServerFrame::Shutdown {
        reason: Some("invalid correlation".to_owned()),
    });
    let write_result = tokio::io::AsyncWriteExt::write_all(stream, &response).await;
    if let Some(operation) = operation {
        operation.complete(
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(RPC_ERROR),
        );
    }
    drop(write_result);
}

pub async fn run_daemon(
    initial_agent: String,
    launch_config: CapsuleConfig,
    telemetry: &mut crate::telemetry::FlushGuard,
) -> Result<()> {
    crate::pid1::install_sigchld_reaper();
    run_daemon_loop(
        initial_agent,
        launch_config,
        telemetry,
        Path::new(socket::SOCKET_PATH),
    )
    .await
}

#[cfg(test)]
pub(crate) async fn run_daemon_for_test(
    initial_agent: String,
    launch_config: CapsuleConfig,
    telemetry: &mut crate::telemetry::FlushGuard,
    socket_path: &Path,
) -> Result<()> {
    run_daemon_loop(initial_agent, launch_config, telemetry, socket_path).await
}

pub(crate) fn spawn_boot_tabs(
    mux: &mut Multiplexer,
    pending: &mut Vec<SpawnRequest>,
) -> Option<anyhow::Error> {
    let boot = std::mem::take(pending);
    for request in boot {
        if let Err(err) = mux.spawn_request(request, &[]) {
            return Some(err);
        }
    }
    None
}
