// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use std::os::unix::fs::symlink;

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageProjectionRefreshStateV1, UsageProjectionSchemaV1,
    UsageProjectionV1, UsageRefreshPhase,
};

use super::*;

mod support;
use support::*;
mod case_01;
