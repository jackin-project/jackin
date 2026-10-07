// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! CLI gates, protobuf scan, and `JSONL` IO.

use std::io::Write;

use std::time::Instant;

use super::CODEX_RPC_LAUNCH_COOLDOWN;

#[derive(Debug, Clone, Default)]
pub struct ManagedCliLaunchGate {
    pub cooldown_until: Option<Instant>,
    pub last_error: Option<String>,
}

impl ManagedCliLaunchGate {
    pub fn can_launch(&self, label: &str, now: Instant) -> Result<(), String> {
        if let Some(until) = self.cooldown_until
            && now < until
        {
            let remaining = until.saturating_duration_since(now).as_secs() / 60;
            return Err(format!(
                "{label} launch cooldown active for {}m: {}",
                remaining.max(1),
                self.last_error
                    .as_deref()
                    .unwrap_or("previous launch failed")
            ));
        }
        Ok(())
    }

    pub fn record_launch_failure(&mut self, message: String) {
        self.cooldown_until = Some(Instant::now() + CODEX_RPC_LAUNCH_COOLDOWN);
        self.last_error = Some(message);
    }

    pub fn record_success(&mut self) {
        self.cooldown_until = None;
        self.last_error = None;
    }
}

#[derive(Debug, Default)]
pub struct ProtobufScan {
    pub fixed32_fields: Vec<Fixed32Field>,
    pub varint_fields: Vec<VarintField>,
}

#[derive(Debug)]
pub struct Fixed32Field {
    pub path: Vec<u64>,
    pub value: f32,
    pub order: usize,
}

#[derive(Debug)]
pub struct VarintField {
    pub path: Vec<u64>,
    pub value: u64,
}

impl ProtobufScan {
    pub fn merge(&mut self, other: Self) {
        self.fixed32_fields.extend(other.fixed32_fields);
        self.varint_fields.extend(other.varint_fields);
    }
}

pub fn looks_like_protobuf_payload(data: &[u8]) -> bool {
    let Some(first) = data.first() else {
        return false;
    };
    let field_number = first >> 3;
    let wire_type = first & 0x07;
    field_number > 0 && matches!(wire_type, 0 | 1 | 2 | 5)
}

pub fn read_varint(data: &[u8], index: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    let mut shift = 0u32;
    while *index < data.len() && shift < 64 {
        let byte = data[*index];
        *index += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
    }
    None
}

pub fn write_json_line(
    stdin: &mut impl Write,
    payload: &serde_json::Value,
    encode_context: &str,
    write_context: &str,
) -> Result<(), String> {
    serde_json::to_writer(&mut *stdin, payload)
        .map_err(|err| format!("{encode_context}: {err}"))?;
    stdin
        .write_all(b"\n")
        .and_then(|()| stdin.flush())
        .map_err(|err| format!("{write_context}: {err}"))
}
