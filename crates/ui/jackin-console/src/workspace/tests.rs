// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `workspace`.

use super::*;

use jackin_config::fixtures::config_with_agents as config_with_agents_for_override;

use jackin_config::{WorkspaceConfig, WorkspaceRoleOverride};

mod support;
use support::*;
mod case_01;
