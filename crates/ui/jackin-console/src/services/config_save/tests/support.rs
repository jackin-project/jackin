// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}

pub(super) fn mount(src: &str, dst: &str) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    }
}
