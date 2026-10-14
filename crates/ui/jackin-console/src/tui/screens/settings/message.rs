// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::model::AccountScanOutcome;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsMessage {
    FocusTabBar,
    FocusContent,
    RequestAccountScan,
    AccountScanCompleted {
        generation: u64,
        result: Result<AccountScanOutcome, String>,
    },
    CancelAccountScan,
}
