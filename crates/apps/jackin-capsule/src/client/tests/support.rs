// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn account(
    provider: &str,
    status: &str,
    source: &str,
    confidence: &str,
) -> AccountUsageSnapshotView {
    AccountUsageSnapshotView {
        provider: provider.to_owned(),
        account_label: format!("{provider} account"),
        source: source.to_owned(),
        confidence: confidence.to_owned(),
        window_kind: "Session".to_owned(),
        used_amount: Some(63),
        used_unit: Some("percent".to_owned()),
        limit_amount: Some(100),
        limit_unit: Some("percent".to_owned()),
        resets_at: Some(1_781_186_000),
        fetched_at: 1_781_185_680,
        expires_at: None,
        status: status.to_owned(),
        last_error: None,
    }
}

pub(super) fn short_socket_path(tmp: &TempDir, file_name: &str) -> PathBuf {
    tmp.path().join(file_name)
}
