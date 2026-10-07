// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use base64::Engine as _;
use jackin_protocol::control::{
    FocusedUsageView, StatusSlot, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::*;

use std::fs;
use std::path::Path;

mod support;
use support::*;
mod case_02;
mod case_03;
mod case_06;
mod case_07;
mod case_08;
mod case_09;
mod case_10;
mod case_11;
mod case_12;
mod case_14;
mod case_15;
