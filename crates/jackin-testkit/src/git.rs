// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::process::Command;

const GIT_PREFIX: &[u8] = b"GIT_";

#[must_use]
pub fn git_command() -> Command {
    let mut command = Command::new("/usr/bin/git");
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(GIT_PREFIX) {
            command.env_remove(name);
        }
    }
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    command
}
