// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Telemetry-instrumented process spawn/exec wrappers.
//!
//! Moved to [`jackin_runtime_process_telemetry::process_telemetry`]; this
//! module keeps the `crate::process_telemetry::*` paths stable for existing
//! callers.

pub(crate) use jackin_runtime_process_telemetry::process_telemetry::*;
