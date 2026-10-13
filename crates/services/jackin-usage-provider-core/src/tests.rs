// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, Money, QuotaBucketView, StatusSlot, UsageConfidence,
    UsageProviderTab, UsageSeverity, UsageSnapshotStatus, UsageSource,
};

use super::*;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
mod case_06;
