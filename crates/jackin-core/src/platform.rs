// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Platform {
    LinuxArm64,
    LinuxAmd64,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.docker_arg())
    }
}

impl Platform {
    pub fn host_default() -> Platform {
        if cfg!(target_arch = "aarch64") {
            Platform::LinuxArm64
        } else {
            Platform::LinuxAmd64
        }
    }

    pub fn docker_arg(self) -> &'static str {
        match self {
            Platform::LinuxArm64 => "linux/arm64",
            Platform::LinuxAmd64 => "linux/amd64",
        }
    }

    pub fn target_arch(self) -> &'static str {
        match self {
            Platform::LinuxArm64 => "arm64",
            Platform::LinuxAmd64 => "amd64",
        }
    }
}
