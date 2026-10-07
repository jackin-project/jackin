// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use std::fs;

use std::sync::atomic::{AtomicUsize, Ordering};

use std::sync::{Arc, Mutex};

use std::time::Duration;

use jackin_protocol::control::{Money, UsageConfidence, UsageSeverity, UsageSource};

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageFreshnessPhaseV1, UsageIdentityKindV1, UsageLifecycleV1,
    UsageMetricValueV1, UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageQuotaStateV1,
};

use super::*;

use jackin_usage_coordinator::{
    AccountStateEnvelope, AccountStateStore, ProviderProbeOutcome, StateStoreError,
    UsageCoordinatorConfig, UsageProviderExecutor,
};

mod support;
use support::*;
mod case_01;
mod case_02;
