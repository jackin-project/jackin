// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! PTY telemetry, exit classification, and fixture capture.

use super::{pty_exit_error_type, pty_exit_reason};

use anyhow::Result;
use std::sync::Mutex;

pub(crate) fn capture_pty_fixture_bytes(bytes: &[u8]) {
    use std::io::Write as _;
    use std::sync::OnceLock;

    static CAPTURE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
    let capture = CAPTURE.get_or_init(|| {
        let path = std::env::var_os("JACKIN_PTY_FIXTURE_CAPTURE")?;
        let file = std::fs::File::create(path).ok()?;
        Some(Mutex::new(file))
    });
    if let Some(capture) = capture
        && let Ok(mut file) = capture.lock()
    {
        drop(file.write_all(bytes));
        drop(file.flush());
    }
}

pub(crate) fn record_terminal_bytes(
    direction: jackin_telemetry::schema::enums::StreamDirection,
    bytes: usize,
) {
    let attrs = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::STREAM_DIRECTION,
        value: jackin_telemetry::Value::Str(direction.as_str()),
    }];
    let amount = u64::try_from(bytes).unwrap_or(u64::MAX);
    let _counter_result =
        jackin_telemetry::counter(&jackin_telemetry::metric::TERMINAL_BYTES).add(amount, &attrs);
}

pub(crate) fn emit_pty_spawn(agent: Option<&str>, conversation_id: Option<&str>) {
    use jackin_telemetry::{Attr, FieldSet, Value};
    let mut attrs = Vec::with_capacity(2);
    if let Some(agent) = agent {
        attrs.push(Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::GEN_AI_AGENT_NAME,
            value: Value::Str(agent),
        });
    }
    if let Some(conversation_id) = conversation_id {
        attrs.push(Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::GEN_AI_CONVERSATION_ID,
            value: Value::Str(conversation_id),
        });
    }
    let _event_result = jackin_telemetry::emit_event(
        &jackin_telemetry::event::PTY_SPAWN,
        FieldSet::new(&attrs, None),
    );
}

pub(crate) fn emit_pty_exit(
    agent: Option<&str>,
    conversation_id: Option<&str>,
    status: Result<&portable_pty::ExitStatus, &std::io::Error>,
    cancelled: bool,
) {
    use jackin_telemetry::{Attr, FieldSet, Value};
    let reason = pty_exit_reason(status, cancelled);
    let mut attrs = vec![Attr {
        key: jackin_telemetry::schema::attrs::PTY_EXIT_REASON,
        value: Value::Str(reason.as_str()),
    }];
    if let Some(error_type) = pty_exit_error_type(reason) {
        attrs.push(Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
            value: Value::Str(error_type.as_str()),
        });
    }
    if let Some(status) = status.ok()
        && !status.success()
        && status.signal().is_none()
    {
        attrs.push(Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
            value: Value::I64(i64::from(status.exit_code())),
        });
    }
    if let Some(agent) = agent {
        attrs.push(Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::GEN_AI_AGENT_NAME,
            value: Value::Str(agent),
        });
    }
    if let Some(conversation_id) = conversation_id {
        attrs.push(Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::GEN_AI_CONVERSATION_ID,
            value: Value::Str(conversation_id),
        });
    }
    let _event_result = jackin_telemetry::emit_event(
        &jackin_telemetry::event::PTY_EXIT,
        FieldSet::new(&attrs, None),
    );
}

pub(crate) fn child_exit_reason(
    status: Result<&portable_pty::ExitStatus, &std::io::Error>,
) -> Option<String> {
    match status {
        Ok(status) if status.success() => None,
        Ok(status) => match status.signal() {
            Some(signal) => Some(format!("session process exited after signal {signal}")),
            None => Some(format!(
                "session process exited with code {}",
                status.exit_code()
            )),
        },
        Err(err) => Some(format!("session process wait failed: {err}")),
    }
}
