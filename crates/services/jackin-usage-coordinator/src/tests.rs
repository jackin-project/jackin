// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::PermissionsExt as _;

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};

use super::*;

mod support_01;
use support_01::*;
mod support_02;
use support_02::*;
mod case_01;
mod case_02;
mod case_03;
