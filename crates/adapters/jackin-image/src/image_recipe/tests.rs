// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use crate::derived_image::AgentInstall;

use jackin_core::Agent;

use jackin_core::RoleSelector;

use jackin_manifest::repo::CachedRepo;

use std::collections::HashMap;

mod support;
use support::*;
mod case_01;
