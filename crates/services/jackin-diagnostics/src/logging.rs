// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
pub(crate) static DEBUG_BUFFER_ACTIVE: AtomicBool = AtomicBool::new(false);
static DEBUG_BUFFER: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
const DEBUG_BUFFER_LIMIT: usize = 2048;

static DEBUG_MODE: AtomicBool = AtomicBool::new(false);
static CONFIG_TELEMETRY_LEVEL: OnceLock<Mutex<Option<TelemetryLevel>>> = OnceLock::new();
static CONFIG_TELEMETRY_CATEGORIES: OnceLock<Mutex<Option<String>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TelemetryLevel {
    Info,
    Debug,
    Trace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetrySink {
    OtlpSpans,
    OtlpLogs,
    Console,
}

pub fn set_debug_mode(enabled: bool) {
    DEBUG_MODE.store(enabled, Ordering::Relaxed);
}

#[must_use]
pub fn is_debug_mode() -> bool {
    DEBUG_MODE.load(Ordering::Relaxed)
}

pub fn telemetry_level(debug: bool) -> TelemetryLevel {
    if let Some(level) = std::env::var("JACKIN_TELEMETRY_LEVEL")
        .ok()
        .and_then(|value| parse_telemetry_level(&value))
    {
        return level;
    }
    config_telemetry_level().unwrap_or({
        if debug {
            TelemetryLevel::Debug
        } else {
            TelemetryLevel::Info
        }
    })
}

#[must_use]
pub fn sink_level(sink: TelemetrySink, debug: bool) -> TelemetryLevel {
    let env_key = match sink {
        TelemetrySink::OtlpSpans => "JACKIN_TELEMETRY_OTLP_SPANS_LEVEL",
        TelemetrySink::OtlpLogs => "JACKIN_TELEMETRY_OTLP_LOGS_LEVEL",
        TelemetrySink::Console => "JACKIN_TELEMETRY_CONSOLE_LEVEL",
    };
    std::env::var(env_key)
        .ok()
        .and_then(|value| parse_telemetry_level(&value))
        .unwrap_or_else(|| telemetry_level(debug))
}

#[must_use]
pub fn telemetry_level_name(level: TelemetryLevel) -> &'static str {
    match level {
        TelemetryLevel::Info => "info",
        TelemetryLevel::Debug => "debug",
        TelemetryLevel::Trace => "trace",
    }
}

pub fn set_config_telemetry(level: Option<TelemetryLevel>, categories: &[String]) {
    *CONFIG_TELEMETRY_LEVEL
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = level;
    *CONFIG_TELEMETRY_CATEGORIES
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = if categories.is_empty() {
        None
    } else {
        Some(categories.join(","))
    };
}

fn config_telemetry_level() -> Option<TelemetryLevel> {
    *CONFIG_TELEMETRY_LEVEL
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn config_telemetry_categories() -> Option<String> {
    CONFIG_TELEMETRY_CATEGORIES
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

pub fn parse_telemetry_level(value: &str) -> Option<TelemetryLevel> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "info" => Some(TelemetryLevel::Info),
        "debug" => Some(TelemetryLevel::Debug),
        "trace" => Some(TelemetryLevel::Trace),
        _ => None,
    }
}

pub(crate) fn debug_capture_enabled(category: &str, legacy_debug: bool) -> bool {
    let level = std::env::var("JACKIN_TELEMETRY_LEVEL")
        .ok()
        .and_then(|value| parse_telemetry_level(&value))
        .or_else(config_telemetry_level)
        .unwrap_or(if legacy_debug {
            TelemetryLevel::Debug
        } else {
            TelemetryLevel::Info
        });
    let categories = std::env::var("JACKIN_TELEMETRY_CATEGORIES")
        .ok()
        .or_else(config_telemetry_categories);
    level >= TelemetryLevel::Debug && telemetry_category_enabled(category, categories.as_deref())
}

fn telemetry_category_enabled(category: &str, categories_env: Option<&str>) -> bool {
    categories_env
        .filter(|raw| !raw.trim().is_empty())
        .is_none_or(|raw| {
            raw.split(',')
                .map(normalize_telemetry_category)
                .any(|candidate| {
                    candidate == "*" || candidate == normalize_telemetry_category(category)
                })
        })
}

fn normalize_telemetry_category(category: &str) -> String {
    category
        .trim()
        .to_ascii_lowercase()
        .replace(['_', ' '], "-")
}

fn debug_buffer() -> &'static Mutex<Vec<String>> {
    DEBUG_BUFFER.get_or_init(|| Mutex::new(Vec::new()))
}

fn stderr_line(line: &str) {
    let mut stderr = std::io::stderr().lock();
    drop(write_redacted_line(&mut stderr, line));
}

fn write_redacted_line(output: &mut impl std::io::Write, line: &str) -> std::io::Result<()> {
    writeln!(output, "{}", crate::redact::redact_text(line))
}

pub(crate) fn should_tee_debug_to_stderr() -> bool {
    !DEBUG_BUFFER_ACTIVE.load(Ordering::Relaxed) && !crate::terminal::rich_terminal_owned()
}

pub(crate) fn drain_debug_buffer() -> Vec<String> {
    let mut guard = debug_buffer()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::mem::take(&mut *guard)
}

pub fn begin_debug_buffering() {
    DEBUG_BUFFER_ACTIVE.store(true, Ordering::Relaxed);
}

pub fn end_debug_buffering() {
    DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
    for line in drain_debug_buffer() {
        stderr_line(&line);
    }
}

pub fn drain_debug_buffer_for_test() -> Vec<String> {
    DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
    drain_debug_buffer()
}

pub fn emit_debug_line(category: &str, message: &str) {
    let line = format_debug_line(category, message);
    if crate::run::active_debug(category, &line) {
        if should_tee_debug_to_stderr() {
            stderr_line(&line);
        }
        return;
    }

    if crate::run::active_run().is_some() {
        return;
    }
    if DEBUG_BUFFER_ACTIVE.load(Ordering::Relaxed) {
        let mut guard = debug_buffer()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard.len() >= DEBUG_BUFFER_LIMIT {
            let keep_from = guard.len() / 2;
            guard.drain(..keep_from);
        }
        guard.push(line);
    } else {
        stderr_line(&line);
    }
}

pub fn emit_compact_line(kind: &str, line: &str) {
    let line = crate::redact::redact_text(line);
    if let Some(run) = crate::run::active_run() {
        run.compact(kind, &line);
    }
    emit_operator_notice(&line);
}

pub fn emit_operator_notice(line: &str) {
    if crate::terminal::rich_terminal_owned() {
        buffer_pending_notice(line);
    } else {
        stderr_line(line);
    }
}

pub fn emit_teardown_notice(line: &str) {
    stderr_line(line);
}

fn buffer_pending_notice(line: &str) {
    let mut guard = debug_buffer()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if guard.len() >= DEBUG_BUFFER_LIMIT {
        let keep_from = guard.len() / 2;
        guard.drain(..keep_from);
    }
    guard.push(crate::redact::redact_text(line).into_owned());
}

#[must_use]
pub fn format_debug_line(category: &str, message: &str) -> String {
    crate::redact::redact_text(&format!("[jackin debug {category}] {message}")).into_owned()
}

#[cfg(test)]
mod tests;
