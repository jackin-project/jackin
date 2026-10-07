// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Codex` RPC transport.

use jackin_usage_provider_core::ProviderError;
use jackin_usage_provider_core::{
    CODEX_RPC_INIT_TIMEOUT, CODEX_RPC_REQUEST_TIMEOUT, ChildOperation, ManagedCliLaunchGate,
    complete_external_rpc, external_rpc_operation, write_json_line,
};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::{CodexRpcAccountResponse, CodexRpcRateLimitsResponse, CodexRpcUsage};

pub(crate) fn decode_codex_rpc_usage(
    limits_value: serde_json::Value,
    account_value: Option<serde_json::Value>,
) -> Result<CodexRpcUsage, ProviderError> {
    let limits =
        serde_json::from_value::<CodexRpcRateLimitsResponse>(limits_value).map_err(|err| {
            ProviderError::from(format!("Codex app-server rate limit decode failed: {err}"))
        })?;
    // The account label is non-essential, so a decode mismatch (unknown
    // tag, shape drift) degrades to no label rather than failing
    // rate-limit collection.
    let account = account_value
        .and_then(|value| serde_json::from_value::<CodexRpcAccountResponse>(value).ok());
    Ok(CodexRpcUsage::from_rpc(limits, account))
}

pub(crate) fn fetch_codex_rpc_usage(
    gate: &mut ManagedCliLaunchGate,
) -> Result<CodexRpcUsage, ProviderError> {
    gate.can_launch("Codex app-server", Instant::now())?;
    let process = ChildOperation::begin("codex");
    let mut child = match Command::new("codex")
        .args(["-s", "read-only", "-a", "untrusted", "app-server"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            process.spawn_failed();
            let message = format!("codex app-server failed to start: {err}");
            gate.record_launch_failure(message.clone());
            return Err(ProviderError::from(message));
        }
    };

    let Some(mut stdin) = child.stdin.take() else {
        process.fail_managed_io(&mut child);
        return Err(ProviderError::from(
            "codex app-server stdin unavailable".to_owned(),
        ));
    };
    let Some(stdout) = child.stdout.take() else {
        process.fail_managed_io(&mut child);
        return Err(ProviderError::from(
            "codex app-server stdout unavailable".to_owned(),
        ));
    };
    let (tx, rx) = mpsc::channel();
    let reader = jackin_telemetry::spawn::thread_stream("codex.stdout", move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let result: Result<CodexRpcUsage, ProviderError> = (|| {
        drop(codex_rpc_request(
            &mut stdin,
            &rx,
            1,
            "initialize",
            serde_json::json!({
                "clientInfo": {
                    "name": "jackin-capsule",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
            CODEX_RPC_INIT_TIMEOUT,
        )?);
        codex_rpc_notification(&mut stdin, "initialized")?;
        let limits_value = codex_rpc_request(
            &mut stdin,
            &rx,
            2,
            "account/rateLimits/read",
            serde_json::json!({}),
            CODEX_RPC_REQUEST_TIMEOUT,
        )?;
        // The account label is non-essential, so a typed RPC failure degrades to
        // no label rather than failing rate-limit collection.
        let account_value = codex_rpc_request(
            &mut stdin,
            &rx,
            3,
            "account/read",
            serde_json::json!({}),
            CODEX_RPC_REQUEST_TIMEOUT,
        )
        .ok();
        decode_codex_rpc_usage(limits_value, account_value)
    })();

    drop(stdin);
    let reaped = ChildOperation::reap_managed(&mut child);
    let reader_joined = reader.join().is_ok();
    process.finish_managed(reaped && reader_joined);

    if result.is_ok() {
        gate.record_success();
    } else if let Err(error) = &result {
        gate.record_launch_failure(error.message().to_owned());
    }
    result
}

pub(crate) fn codex_rpc_request(
    stdin: &mut impl Write,
    rx: &mpsc::Receiver<String>,
    id: i64,
    method: &str,
    params: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let operation = external_rpc_operation(
        jackin_telemetry::schema::enums::RpcSystemName::CodexAppServer,
        method,
    );
    let started = Instant::now();
    let result = (|| {
        let payload = serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        });
        write_json_line(
            stdin,
            &payload,
            "Codex app-server request encode failed",
            "Codex app-server request write failed",
        )?;
        loop {
            let remaining = timeout
                .checked_sub(started.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));
            if remaining.is_zero() {
                return Err(format!("Codex app-server timed out waiting for {method}"));
            }
            let line = rx
                .recv_timeout(remaining)
                .map_err(|_| format!("Codex app-server timed out waiting for {method}"))?;
            let value: serde_json::Value = serde_json::from_str(&line)
                .map_err(|err| format!("Codex app-server response decode failed: {err}"))?;
            if value.get("id").and_then(serde_json::Value::as_i64) != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown error");
                return Err(format!("Codex app-server {method} failed: {message}"));
            }
            return value
                .get("result")
                .cloned()
                .ok_or_else(|| format!("Codex app-server {method} response missing result"));
        }
    })();
    complete_external_rpc(operation, &result, started.elapsed() >= timeout);
    result
}

pub(crate) fn codex_rpc_notification(stdin: &mut impl Write, method: &str) -> Result<(), String> {
    let operation = external_rpc_operation(
        jackin_telemetry::schema::enums::RpcSystemName::CodexAppServer,
        method,
    );
    let payload = serde_json::json!({
        "method": method,
        "params": {},
    });
    let result = write_json_line(
        stdin,
        &payload,
        "Codex app-server notification encode failed",
        "Codex app-server notification write failed",
    );
    complete_external_rpc(operation, &result, false);
    result
}
