// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use super::*;

use jackin_core::{Agent, WorkspaceName};

use jackin_config::{
    AccountCredential, AgentConfiguration, AiProvider, AppConfig, CURRENT_WORKSPACE_VERSION,
    KeepAwakeConfig, MountConfig, MountIsolation, RoleSource, WorkspaceConfig,
};

mod support;
use support::*;
mod case_01;
mod case_02;
