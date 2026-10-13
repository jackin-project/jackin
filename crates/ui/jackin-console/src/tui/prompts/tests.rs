// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use crate::services::launch::accounts_for_launch;

use jackin_config::{
    AccountConfig, AccountCredential, AgentConfiguration, AiProvider, WorkspaceConfig,
    WorkspaceRoleOverride,
};

use jackin_core::{Agent, EnvValue, WorkspaceName};

use std::collections::BTreeMap;

mod support;
use support::*;
mod case_01;
mod case_02;
