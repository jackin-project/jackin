// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[derive(Default)]
struct RoleEnv {
    env: BTreeMap<String, &'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestAuthKind {
    Claude,
    Github,
}

