// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use jackin_config::{MountConfig, MountHealReport, ResolvedWorkspace};

use jackin_core::{Agent, AuthForwardMode, MountIsolation};

use super::*;

use jackin_instance::{
    AgentRuntimeState, GithubProvisionOutcome, ProvisionedAuth, ProvisionedInstanceAuth, RoleState,
};

mod support;
use support::*;
mod case_01;
