// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `role`.

use super::ConsoleArgs;
use super::HardlineArgs;
use super::LoadArgs;
use super::RoleCommand;
use super::RoleCreateArgs;
use super::RolePublishLabelsArgs;
use super::RoleRepoPathArgs;
use crate::cli::{Cli, Command};

use clap::Parser;

mod support;
use support::*;
mod case_01;
mod case_02;
