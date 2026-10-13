// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn mount(src: &str, dst: &str) -> MountConfig {
    MountConfig {
        src: src.to_owned(),
        dst: dst.to_owned(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    }
}

pub(super) fn workspace(workdir: &str, mounts: Vec<MountConfig>) -> WorkspaceConfig {
    WorkspaceConfig {
        workdir: workdir.to_owned(),
        mounts,
        ..Default::default()
    }
}

pub(super) fn mk(src: &str, dst: &str, ro: bool) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: ro,
        isolation: jackin_core::MountIsolation::Shared,
    }
}
