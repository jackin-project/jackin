// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

// `Dialog` import removed: Dialog type lives in jackin-capsule; using it
// from jackin-usage tests would create a circular dep (Blocker 2 Option A).

use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSnapshotStatus,
    UsageSource,
};

use super::*;

mod support;
use support::*;
mod case_01;
mod case_02;
