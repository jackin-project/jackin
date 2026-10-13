// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn identity() -> crate::LaunchIdentity {
    crate::LaunchIdentity {
        role: "architect".into(),
        agent: "claude".into(),
        target_kind: LaunchTargetKind::Workspace,
        target_label: "demo".into(),
        mounts: Vec::new(),
        image: None,
        container: None,
    }
}
