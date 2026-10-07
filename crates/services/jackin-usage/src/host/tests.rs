// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_usage_provider_core::{
    PercentStyle, ResetStyle, UsageFormatPrefs, estimate_caption, provider_display_label,
};

use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, Money, QuotaBucketView, StatusSlot, UsageConfidence,
    UsageSeverity, UsageSnapshotStatus, UsageSource,
};

use std::sync::Arc;

use std::sync::atomic::AtomicUsize;

use super::broker::{UNIX_SOCKET_PATH_LIMIT, short_socket_alias};

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
mod case_06;
mod case_07;
mod case_08;
mod case_09;
