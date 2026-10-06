// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Settings-screen TUI message vocabulary.
//!
//! Root-crate settings messages still live in `src/console/manager/message.rs`
//! while they carry root-only config and credential types. This module is the
//! screen-local home for root-independent settings messages as the migration
//! continues.

use super::model::AccountScanOutcome;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsMessage {
    FocusTabBar,
    FocusContent,
    /// Operator requested an account scan on the Accounts tab.
    RequestAccountScan,
    /// Scan worker finished. `generation` must match the epoch captured
    /// when the worker spawned; stale completions are ignored.
    AccountScanCompleted {
        generation: u64,
        result: Result<AccountScanOutcome, String>,
    },
    /// Operator abandoned the in-flight scan; orphan its completion.
    CancelAccountScan,
}
