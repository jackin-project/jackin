// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use std::fs;

use std::path::PathBuf;

use std::sync::Arc;

use std::sync::atomic::{AtomicUsize, Ordering};

use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};

use jackin_protocol::usage_broker::{
    UsageCoordinationError, UsageCoordinationErrorKind, UsageCredentialSourceIdentity,
    UsageRefreshPhase, usage_credential_material_fingerprint,
};

use jackin_usage_coordinator::{ProviderProbeOutcome, UsageCapabilitySet, UsageProviderExecutor};

use jackin_usage_host_runtime::host::ensure_usage_broker_with_executor;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
