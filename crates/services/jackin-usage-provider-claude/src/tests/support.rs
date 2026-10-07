// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use super::*;

pub(super) fn keychain_test_scope(is_default: bool) -> jackin_core::ClaudeKeychainScope {
    jackin_core::ClaudeKeychainScope {
        normalized_config_dir: PathBuf::from(if is_default {
            "/home/u/.claude"
        } else {
            "/home/u/.claude-work"
        }),
        service: if is_default {
            "Claude Code-credentials".to_owned()
        } else {
            "Claude Code-credentials-3342f2c7".to_owned()
        },
        is_default,
    }
}

pub(super) const KEYCHAIN_PAYLOAD: &str = r#"{"claudeAiOauth":{"accessToken":"kc-token","subscriptionType":"max","refreshToken":"rt-1"}}"#;

pub(super) fn empty_file_probe() -> ClaudeFileProbe {
    ClaudeFileProbe {
        credential: None,
        origin: None,
        account_email: None,
        organization_type: None,
    }
}
