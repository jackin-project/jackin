// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn launch_env_runner_uses_wider_bounded_timeout() {
    let runner = OpCli::new_launch_env();

    assert_eq!(runner.binary, OP_DEFAULT_BIN);
    assert_eq!(runner.timeout, std::time::Duration::from_mins(2));
    assert_eq!(runner.account, None);
}
