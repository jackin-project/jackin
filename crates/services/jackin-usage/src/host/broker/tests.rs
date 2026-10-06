// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::discovery;
use std::collections::{BTreeMap, BTreeSet};

use std::os::unix::fs::symlink;

use std::path::PathBuf;

use std::sync::atomic::{AtomicUsize, Ordering};

use std::sync::{Arc, Barrier, Mutex};

use std::thread;

use crate::host::{HostSurfaceId, OpaqueCredentialHandle};

use jackin_config::AppConfig;

use jackin_core::{UsageCredentialEnvName, WorkspaceName};

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};

use jackin_protocol::usage_broker::{
    UsageCatalogEntry, UsageCredentialScope, UsageCredentialSourceIdentity,
    UsageCredentialSourceProof, UsageFreshnessPhaseV1, UsageIdentityKindV1,
    UsageProjectionRefreshStateV1, UsageRefreshPhase, usage_credential_material_fingerprint,
};

use super::*;

use crate::host::{ForwardedUsageAccount, ProviderCredentialEnvResolution};

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
mod case_06;
mod case_07;
