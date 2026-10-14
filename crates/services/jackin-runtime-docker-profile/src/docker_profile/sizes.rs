// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::GrantValidationError;

pub fn parse_memory_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let split = s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len());
    let number: u64 = s[..split].trim().parse().ok()?;
    let suffix = s[split..].trim().to_ascii_uppercase();
    let multiplier = match suffix.as_str() {
        "K" | "KB" => KB,
        "M" | "MB" => MB,
        "G" | "GB" => GB,
        "T" | "TB" => GB * 1_024,
        "" => 1,
        _ => return None,
    };
    number.checked_mul(multiplier)
}

pub(crate) const KB: u64 = 1_024;
pub(crate) const MB: u64 = KB * 1_024;
pub(crate) const GB: u64 = MB * 1_024;

pub(crate) fn format_bytes(bytes: u64) -> String {
    if bytes.is_multiple_of(GB) {
        format!("{}G", bytes / GB)
    } else if bytes.is_multiple_of(MB) {
        format!("{}M", bytes / MB)
    } else if bytes.is_multiple_of(KB) {
        format!("{}K", bytes / KB)
    } else {
        format!("{bytes}B")
    }
}

pub(crate) fn parse_size_field(
    errors: &mut Vec<GrantValidationError>,
    field: &'static str,
    value: Option<&str>,
) -> Option<u64> {
    let raw = value?;
    let Some(bytes) = parse_memory_bytes(raw) else {
        errors.push(GrantValidationError::UnparsableSize {
            field,
            value: raw.to_owned(),
        });
        return None;
    };
    if bytes > i64::MAX as u64 {
        errors.push(GrantValidationError::ValueOutOfRange {
            field,
            reason: "exceeds i64::MAX (≈ 8 EiB); use a value ≤ 8 EiB",
        });
    }
    Some(bytes)
}
