// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}

