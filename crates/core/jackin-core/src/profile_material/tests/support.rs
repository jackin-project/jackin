// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn revision_for_test(agent: Agent, raw: &[u8], message: &str) -> Option<String> {
    let result = profile_credential_material_revision(agent, raw);
    assert!(result.is_ok(), "{message}");
    let Ok(revision) = result else {
        return None;
    };
    Some(revision)
}
