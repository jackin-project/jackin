// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Grok` RPC transport and protobuf scan.

use jackin_usage_provider_core::{
    Fixed32Field, ProtobufScan, VarintField, complete_external_rpc, external_rpc_operation,
    home_path, looks_like_protobuf_payload, parse_iso_epoch, read_varint, write_json_line,
};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::GrokWebBillingSnapshot;

pub(crate) fn grok_binary_path() -> PathBuf {
    let home_bin = home_path(".grok/bin/grok");
    if home_bin.is_file() {
        home_bin
    } else {
        PathBuf::from("grok")
    }
}

/// Subscription Bearer [REDACTED] the stored auth file only. This never consults
/// `XAI_API_KEY` / `GROK_DEPLOYMENT_KEY`: ambient inference keys are not
/// consumer-billing auth (see [`resolve_grok_billing_auth`]).
pub(crate) fn grok_bearer_token(auth_path: &Path, now: i64) -> Result<String, String> {
    let text = fs::read_to_string(auth_path).map_err(|err| format!("auth read failed: {err}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|err| format!("auth decode failed: {err}"))?;
    let Some(entries) = value.as_object() else {
        return Err("auth.json root is not an object".to_owned());
    };
    let mut legacy: Option<(&str, &serde_json::Value)> = None;
    for (scope, entry) in entries {
        let is_oidc = scope.starts_with("https://auth.x.ai::");
        let is_legacy = scope == "https://accounts.x.ai/sign-in" || scope.contains("/sign-in");
        if is_legacy {
            legacy = Some((scope, entry));
        }
        if is_oidc && let Some(token) = grok_bearer_token_from_entry(entry, now)? {
            return Ok(token);
        }
    }
    if let Some((_, entry)) = legacy
        && let Some(token) = grok_bearer_token_from_entry(entry, now)?
    {
        return Ok(token);
    }
    Err("no fresh Grok bearer token in auth.json".to_owned())
}

pub(crate) fn grok_bearer_token_from_entry(
    entry: &serde_json::Value,
    now: i64,
) -> Result<Option<String>, String> {
    let Some(token) = entry.get("key").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    if token.is_empty() {
        return Ok(None);
    }
    if let Some(expires_at) = entry.get("expires_at").and_then(serde_json::Value::as_str)
        && let Some(epoch) = parse_iso_epoch(expires_at)
        && epoch <= now
    {
        return Err("Grok bearer token is expired".to_owned());
    }
    Ok(Some(token.to_owned()))
}

pub(crate) fn parse_grok_web_billing_response(
    data: &[u8],
    now: i64,
) -> Result<GrokWebBillingSnapshot, String> {
    let mut payloads = grpc_web_data_frames(data);
    if payloads.is_empty() && looks_like_protobuf_payload(data) {
        payloads.push(data.to_vec());
    }
    if payloads.is_empty() {
        return Err("empty gRPC-web payload".to_owned());
    }
    let mut scan = ProtobufScan::default();
    for payload in payloads {
        scan.merge(scan_protobuf(&payload, 0, Vec::new(), &mut 0));
    }
    let used_percent = scan
        .fixed32_fields
        .iter()
        .filter(|field| {
            field.path.last() == Some(&1)
                && field.value.is_finite()
                && field.value >= 0.0
                && field.value <= 100.0
        })
        .min_by(|left, right| {
            left.path
                .len()
                .cmp(&right.path.len())
                .then_with(|| left.order.cmp(&right.order))
        })
        .map(|field| f64::from(field.value))
        .ok_or_else(|| "usage percent not found in Grok billing protobuf".to_owned())?;
    let reset_at_epoch = scan
        .varint_fields
        .iter()
        .filter(|field| field.value >= 1_700_000_000 && field.value <= 2_100_000_000)
        .filter_map(|field| i64::try_from(field.value).ok().map(|epoch| (field, epoch)))
        .filter(|(_, epoch)| *epoch > now)
        .min_by_key(|(field, epoch)| {
            let preferred = i32::from(field.path != [1, 5, 1]);
            (preferred, *epoch)
        })
        .map(|(_, epoch)| epoch);
    Ok(GrokWebBillingSnapshot {
        used_percent,
        reset_at_epoch,
    })
}

pub(crate) fn grpc_web_data_frames(data: &[u8]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut index = 0;
    while index < data.len() {
        if index + 5 > data.len() {
            break;
        }
        let flags = data[index];
        let length = (usize::from(data[index + 1]) << 24)
            | (usize::from(data[index + 2]) << 16)
            | (usize::from(data[index + 3]) << 8)
            | usize::from(data[index + 4]);
        let start = index + 5;
        let end = start.saturating_add(length);
        if end > data.len() {
            break;
        }
        if flags & 0x80 == 0 {
            frames.push(data[start..end].to_vec());
        }
        index = end;
    }
    frames
}

pub(crate) fn scan_protobuf(
    data: &[u8],
    depth: usize,
    path: Vec<u64>,
    order: &mut usize,
) -> ProtobufScan {
    let mut scan = ProtobufScan::default();
    let mut index = 0;
    while index < data.len() {
        let field_start = index;
        let Some(key) = read_varint(data, &mut index) else {
            index = field_start.saturating_add(1);
            continue;
        };
        if key == 0 {
            index = field_start.saturating_add(1);
            continue;
        }
        let field_number = key >> 3;
        let wire_type = key & 0x07;
        let field_path = {
            let mut next = path.clone();
            next.push(field_number);
            next
        };
        match wire_type {
            0 => {
                if let Some(value) = read_varint(data, &mut index) {
                    scan.varint_fields.push(VarintField {
                        path: field_path,
                        value,
                    });
                } else {
                    index = field_start.saturating_add(1);
                }
            }
            1 => {
                index = index.saturating_add(8).min(data.len());
            }
            2 => {
                let Some(length) =
                    read_varint(data, &mut index).and_then(|v| usize::try_from(v).ok())
                else {
                    index = field_start.saturating_add(1);
                    continue;
                };
                let start = index;
                let end = start.saturating_add(length);
                if end > data.len() {
                    break;
                }
                if depth < 4 {
                    scan.merge(scan_protobuf(
                        &data[start..end],
                        depth + 1,
                        field_path,
                        order,
                    ));
                }
                index = end;
            }
            5 => {
                if index + 4 > data.len() {
                    break;
                }
                let bytes = [
                    data[index],
                    data[index + 1],
                    data[index + 2],
                    data[index + 3],
                ];
                index += 4;
                let value = f32::from_le_bytes(bytes);
                let current_order = *order;
                *order = order.saturating_add(1);
                scan.fixed32_fields.push(Fixed32Field {
                    path: field_path,
                    value,
                    order: current_order,
                });
            }
            _ => {
                index = field_start.saturating_add(1);
            }
        }
    }
    scan
}

pub(crate) fn grok_rpc_request(
    stdin: &mut impl Write,
    rx: &mpsc::Receiver<String>,
    id: i64,
    method: &str,
    params: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let operation = external_rpc_operation(
        jackin_telemetry::schema::enums::RpcSystemName::GrokAcp,
        method,
    );
    let started = Instant::now();
    let result = (|| {
        let payload = grok_rpc_request_payload(id, method, params);
        write_json_line(
            stdin,
            &payload,
            "Grok RPC request encode failed",
            "Grok RPC request write failed",
        )?;
        loop {
            let remaining = timeout
                .checked_sub(started.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));
            if remaining.is_zero() {
                return Err(format!("Grok RPC timed out waiting for {method}"));
            }
            let line = rx
                .recv_timeout(remaining)
                .map_err(|_| format!("Grok RPC timed out waiting for {method}"))?;
            let value: serde_json::Value = serde_json::from_str(&line)
                .map_err(|err| format!("Grok RPC decode failed: {err}"))?;
            if value.get("id").and_then(serde_json::Value::as_i64) != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown error");
                return Err(format!("Grok RPC {method} failed: {message}"));
            }
            return value
                .get("result")
                .cloned()
                .ok_or_else(|| format!("Grok RPC {method} response missing result"));
        }
    })();
    complete_external_rpc(operation, &result, started.elapsed() >= timeout);
    result
}

pub(crate) fn grok_rpc_request_payload(
    id: i64,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}
