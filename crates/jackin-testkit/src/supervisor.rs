// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SupervisorModel {
    Cooperative,
    ExitsOnStart { code: i32 },
    NeverWritesState,
}
