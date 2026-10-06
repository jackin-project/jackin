// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicUsize, Ordering};

use std::sync::{Arc, Condvar, Mutex};

use std::time::{Duration, Instant};

use super::*;

use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, QuotaBucketView, StatusSlot, UsageConfidence,
    UsageSeverity, UsageSnapshotStatus, UsageSource,
};

use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCoordinationErrorKind};

use jackin_usage::coordinator::{ProviderProbeOutcome, UsageProviderExecutor};

use jackin_usage::host::{
    ForwardedUsageAccount, HostProbePolicy, HostRuntimeConfig, UsageBrokerConfig,
    UsageDiscoveryScope, ensure_usage_broker_with_executor, usage_broker_capabilities,
};

use crate::dto::UsageFormatPrefsDto;

mod selected_account_route;

mod support;
use support::*;
mod case_01;
mod case_02;
