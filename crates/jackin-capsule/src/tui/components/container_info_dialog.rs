// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Container info dialog component: shows role, workspace, and agent metadata
//! for the current container.
//!
//! Not responsible for: fetching container metadata (caller populates
//! `ContainerInfoDiagnostics`) or dialog open/close lifecycle.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerInfoDiagnostics {
    pub host_version: String,
    pub run_id: String,
    pub run_log_display: String,
    pub run_log_href: Option<String>,
}

impl Default for ContainerInfoDiagnostics {
    fn default() -> Self {
        Self {
            host_version: "unknown".to_owned(),
            run_id: String::new(),
            run_log_display: "(not set)".to_owned(),
            run_log_href: None,
        }
    }
}
