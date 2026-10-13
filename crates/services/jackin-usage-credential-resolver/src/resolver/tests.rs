// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicUsize, Ordering};

use jackin_protocol::control::{UsageConfidence, UsageSnapshotStatus, UsageSource};
use jackin_protocol::usage_broker::UsageCredentialSourceIdentity;
use jackin_usage_provider_core::{UsageCache, UsageRefreshTarget, capability_matches_surface};

use super::*;
use crate::provider_credential_snapshot;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
