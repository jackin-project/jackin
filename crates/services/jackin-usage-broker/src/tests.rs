// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_discovery as discovery;
use std::collections::{BTreeMap, BTreeSet};

use std::sync::{Arc, Mutex};

use std::time::Duration;

use jackin_usage_host_credentials::OpaqueCredentialHandle;
use jackin_usage_host_presentation::HostSurfaceId;

use jackin_config::AppConfig;

use jackin_core::{UsageCredentialEnvName, WorkspaceName};

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCredentialScope, UsageCredentialSourceIdentity,
    UsageCredentialSourceProof, usage_credential_material_fingerprint,
};

use jackin_usage_coordinator::ProviderProbeOutcome;
use jackin_usage_discovery::{
    UsageDiscoveryScope, ValidatedCredentialBinding, ValidatedCredentialSource,
    ValidatedUsageDiscovery,
};
use jackin_usage_host_credentials::ProviderCredentialEnvResolution;
use jackin_usage_host_credentials::{
    ProviderCredentialEnvResolver, ProviderCredentialRefreshOutcome,
    ProviderCredentialSourceMaterial,
};

use crate::*;

use crate::probe_with_scope;

mod support;
use support::*;
mod case_03;
mod statusline;
