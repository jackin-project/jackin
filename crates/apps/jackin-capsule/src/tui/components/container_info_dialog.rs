// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerInfoDiagnostics {
    pub host_version: String,
    pub invocation_id: String,
}

impl Default for ContainerInfoDiagnostics {
    fn default() -> Self {
        Self {
            host_version: "unknown".to_owned(),
            invocation_id: String::new(),
        }
    }
}
