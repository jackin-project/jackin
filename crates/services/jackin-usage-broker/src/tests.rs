// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_discovery as discovery;
use std::collections::{BTreeMap, BTreeSet};

use std::os::unix::fs::symlink;

use std::sync::atomic::{AtomicUsize, Ordering};

use std::sync::{Arc, Barrier, Mutex};

use std::thread;

use jackin_usage_host_credentials::OpaqueCredentialHandle;
use jackin_usage_host_presentation::HostSurfaceId;

use jackin_config::AppConfig;

use jackin_core::{UsageCredentialEnvName, WorkspaceName};

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCredentialScope, UsageCredentialSourceIdentity,
    UsageCredentialSourceProof, UsageFreshnessPhaseV1, UsageIdentityKindV1,
    UsageProjectionRefreshStateV1, UsageRefreshPhase, usage_credential_material_fingerprint,
};

use jackin_protocol::usage_broker::{
    USAGE_BROKER_PROTOCOL_VERSION, UsageBrokerOperation, UsageBrokerRequest, UsageBrokerResponse,
    UsageCoordinationError, UsageCoordinationErrorKind,
};
use jackin_usage_coordinator::{ProviderProbeOutcome, UsageProviderExecutor};
use jackin_usage_discovery::{
    UsageDiscoveryScope, ValidatedCredentialBinding, ValidatedCredentialSource,
    ValidatedUsageDiscovery,
};
use jackin_usage_host_credentials::{
    ProviderCredentialEnvResolver, ProviderCredentialRefreshOutcome,
    ProviderCredentialSourceMaterial,
};
use std::fs;
use std::io::Write as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::*;

use crate::{
    probe_with_scope, provider_probe_outcome, provider_probe_outcome_with_rate_limit,
    write_with_deadline,
};
use jackin_usage_host_credentials::{ForwardedUsageAccount, ProviderCredentialEnvResolution};

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
mod case_10;
