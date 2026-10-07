// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use jackin_protocol::control::{FocusedAccountHeader, FocusedUsageView, UsageSource};

use jackin_usage_host_accounts::{
    AccountCatalogEntry, AccountLifecycle, AccountProvenance, CanonicalAccountIdentity,
    CanonicalAccountSubject,
};

use crate::*;

use std::collections::HashMap;

use std::collections::HashSet;

use jackin_protocol::control::{UsageActivityKind, UsageDetailRowKind};

use jackin_protocol::usage_broker::{UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1};

use jackin_usage_host_presentation::HostSurfaceId;

use jackin_usage_provider_core::{
    CachedUsage, UsageSurface, UsageViewInput, enrich_provider_tabs, mark_active_tab,
    provider_display_label, provider_tabs, refresh_cached_updated_label, timed_bucket,
    usage_bucket_presentation, usage_detail_presentation, usage_identity_presentation, usage_view,
};

use jackin_console::tui::screens::usage::{
    UsageScreenState, UsageWindow, freshness_age_label, group_freshness_label,
};

mod support_01;
use support_01::*;
mod support_02;
use support_02::*;
mod support_03;
use support_03::*;
mod support_04;
use support_04::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
mod case_06;
