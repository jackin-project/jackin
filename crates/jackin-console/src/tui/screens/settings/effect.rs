// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Settings-screen TUI effect vocabulary.
//!
//! Config persistence and credential validation are executed by root-crate
//! effect adapters. Root-independent settings effects belong here as they are
//! introduced.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEffect {
    /// Run account discovery on a worker thread (blocking filesystem /
    /// Keychain I/O — never on the UI thread), then dispatch
    /// `AccountScanCompleted` carrying this generation.
    StartAccountScan { generation: u64 },
}
