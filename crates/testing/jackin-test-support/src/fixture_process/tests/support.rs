// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[expect(
    clippy::disallowed_methods,
    reason = "self-test drives the fake binary on the test thread; never a render/runtime path"
)]
pub(super) fn run(args: &[&str], fake: &FakeBinary) -> Result<(String, String, i32)> {
    let output = fake.command().args(args).output()?;
    Ok((
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(-1),
    ))
}
