// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) fn violation(path: &str) -> (String, String) {
    (path.to_owned(), "inline test module".to_owned())
}
