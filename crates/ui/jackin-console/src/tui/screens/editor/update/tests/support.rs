// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[derive(Default)]
pub(super) struct RoleEnv {
    pub(super) env: BTreeMap<String, &'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TestAuthKind {
    Claude,
    Github,
}
