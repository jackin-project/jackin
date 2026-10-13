// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::classify_executable;
use crate::schema::enums::ProcessExecutableName;

#[test]
fn classifier_uses_basename_and_closed_fallback() {
    assert_eq!(
        classify_executable(Path::new("/usr/bin/git")),
        ProcessExecutableName::Git
    );
    assert_eq!(
        classify_executable(Path::new("operator-private-tool")),
        ProcessExecutableName::Other
    );
}

use std::path::Path;
