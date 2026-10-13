// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::env_rows;
use super::resolve_env_value_for_cli;
use super::resolve_env_value_for_cli_with_runner;
use super::unresolved_op_ref;

use jackin_core::EnvValue;

use std::collections::BTreeMap;

mod support;
use support::*;
mod case_01;
