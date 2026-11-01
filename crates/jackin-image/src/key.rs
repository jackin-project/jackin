// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Sha256;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageKey(Sha256);
