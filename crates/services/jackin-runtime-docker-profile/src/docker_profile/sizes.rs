// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Memory/size parsing and formatting.

use super::GrantValidationError;

/// Parse a human-readable byte size into a byte count. Case-insensitive suffix.
///
/// Accepts K/M/G/T (with or without trailing B) and bare numeric bytes.
/// Examples: `"512M"`, `"4G"`, `"16G"`, `"2048K"`, `"2T"`.
///
/// Returns `None` if the string is empty, the numeric part cannot be parsed,
/// or the suffix is unrecognized.
pub fn parse_memory_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // Find where the numeric part ends.
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

/// Parse a `--memory`-style size grant and range-check it for the Docker/Bollard
/// `i64` boundary, pushing the matching validation error on failure. Returns the
/// parsed bytes (even when out of range) so cross-field comparisons can proceed.
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
