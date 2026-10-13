// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn key(endpoint: &str) -> ChannelKey {
    ChannelKey::new(endpoint, Duration::from_secs(1), &TlsConfig::default())
}
