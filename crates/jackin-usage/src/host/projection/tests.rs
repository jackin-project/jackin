// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use jackin_protocol::control::{FocusedAccountHeader, FocusedUsageView, UsageSource};

use super::super::accounts::{
    AccountCatalogEntry, AccountLifecycle, AccountProvenance, CanonicalAccountIdentity,
    CanonicalAccountSubject,
};
use super::*;

fn sorted(locale: &str, labels: &[&str]) -> Vec<String> {
    let locale = locale.parse::<Locale>().expect("test locale");
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Secondary);
    let collator = Collator::try_new(locale.into(), options).expect("test collator");
    let mut labels = labels.iter().map(ToString::to_string).collect::<Vec<_>>();
    labels.sort_by(|left, right| collator.compare(left, right));
    labels
}

fn bucket(label: &str) -> QuotaBucketView {
    QuotaBucketView {
        label: label.into(),
        used_label: None,
        limit_label: None,
        remaining_percent: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}

fn view_with_buckets(
    status: UsageSnapshotStatus,
    buckets: Vec<QuotaBucketView>,
) -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: None,
        focused_provider: None,
        account: FocusedAccountHeader {
            provider_label: "Codex".into(),
            account_label: "work@example.test".into(),
            username: None,
            plan_label: None,
            credential_origin: None,
        },
        buckets,
        status,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: 1_800_000_000,
        updated_label: "now".into(),
        status_bar_label: "ok".into(),
        tabs: Vec::new(),
        last_error: None,
    }
}

fn catalog_entry(view: FocusedUsageView, plan_label: Option<&str>) -> AccountCatalogEntry {
    AccountCatalogEntry {
        identity: CanonicalAccountIdentity {
            surface: HostSurfaceId::Codex,
            subject: CanonicalAccountSubject::ProviderId("codex-account-1".into()),
        },
        account_key: "codex:default".into(),
        account_label: "work@example.test".into(),
        username: None,
        plan_label: plan_label.map(str::to_owned),
        provenance: BTreeSet::from([AccountProvenance::LiveHost]),
        discovery_provenance: BTreeSet::from(["live".to_owned()]),
        lifecycle: AccountLifecycle::Current,
        view,
        fetched_at_epoch: 1_800_000_000,
    }
}

#[test]
fn window_projection_preserves_raw_overage_from_money_ratio() {
    let mut spend = bucket("Extra usage");
    spend.status_slot = Some(StatusSlot::Spend);
    spend.used_money = Some(Money::new(12_000, "USD", 2));
    spend.limit_money = Some(Money::new(10_000, "USD", 2));
    spend.used_label = Some("$120.00 of $100.00".into());
    let window = project_window("canon-1", &spend, 0).unwrap();
    assert_eq!(window.used_percent.map(UsagePercent::get), Some(100));
    assert_eq!(window.used_raw_percent, Some(120));
    assert_eq!(window.remaining_percent, None);
    assert_eq!(window.quota_state, UsageQuotaStateV1::Exhausted);
    window.validate(0).unwrap();
}

#[test]
fn window_projection_keeps_checked_math_without_wrap_or_fabrication() {
    let mut huge = bucket("Huge");
    huge.used_money = Some(Money::new(i64::MAX, "USD", 2));
    huge.limit_money = Some(Money::new(1, "USD", 2));
    let window = project_window("canon-1", &huge, 0).unwrap();
    assert_eq!(window.used_percent.map(UsagePercent::get), Some(100));
    assert_eq!(window.used_raw_percent, Some(i32::MAX));
    window.validate(0).unwrap();

    let mut mismatched = bucket("Mismatched");
    mismatched.used_money = Some(Money::new(50_00, "USD", 2));
    mismatched.limit_money = Some(Money::new(10_000, "SGD", 2));
    let window = project_window("canon-1", &mismatched, 0).unwrap();
    assert_eq!(window.used_percent, None);
    assert_eq!(window.used_raw_percent, None);
    window.validate(0).unwrap();

    let mut zero_cap = bucket("Zero cap");
    zero_cap.used_money = Some(Money::new(1, "USD", 2));
    zero_cap.limit_money = Some(Money::new(0, "USD", 2));
    let window = project_window("canon-1", &zero_cap, 0).unwrap();
    assert_eq!(window.used_percent, None);
    window.validate(0).unwrap();
}

#[test]
fn quota_state_keeps_permission_unknown_and_exhausted_distinct() {
    let mut login = bucket("Login");
    login.status = UsageSnapshotStatus::NeedsLogin;
    assert_eq!(quota_state(&login), UsageQuotaStateV1::NoPermission);

    let mut secret = bucket("Secret");
    secret.status = UsageSnapshotStatus::NeedsSecret;
    assert_eq!(quota_state(&secret), UsageQuotaStateV1::NoPermission);

    let mut unsupported = bucket("Unsupported");
    unsupported.status = UsageSnapshotStatus::Unsupported;
    assert_eq!(quota_state(&unsupported), UsageQuotaStateV1::Unsupported);

    let mut unavailable = bucket("Unavailable");
    unavailable.status = UsageSnapshotStatus::Unavailable;
    assert_eq!(quota_state(&unavailable), UsageQuotaStateV1::Unavailable);

    let mut error = bucket("Error");
    error.status = UsageSnapshotStatus::Error;
    assert_eq!(quota_state(&error), UsageQuotaStateV1::Error);

    let empty = bucket("Empty");
    assert_eq!(quota_state(&empty), UsageQuotaStateV1::Unknown);

    let mut exhausted = bucket("Exhausted");
    exhausted.remaining_percent = Some(0);
    assert_eq!(quota_state(&exhausted), UsageQuotaStateV1::Exhausted);

    let mut available = bucket("Available");
    available.remaining_percent = Some(57);
    assert_eq!(quota_state(&available), UsageQuotaStateV1::Available);

    let mut warn = bucket("Warn");
    warn.remaining_percent = Some(10);
    warn.severity = UsageSeverity::Warn;
    assert_eq!(quota_state(&warn), UsageQuotaStateV1::Warning);

    let mut danger = bucket("Danger");
    danger.remaining_percent = Some(10);
    danger.severity = UsageSeverity::Danger;
    assert_eq!(quota_state(&danger), UsageQuotaStateV1::Exhausted);
}

#[test]
fn account_projects_window_spend_and_plan_groups() {
    let mut weekly = bucket("Weekly");
    weekly.remaining_percent = Some(57);
    weekly.resets_at = Some(1_800_100_000);
    weekly.reset_label = Some("Resets soon".into());
    weekly.status_slot = Some(StatusSlot::Weekly);
    let mut spend = bucket("Extra usage");
    spend.status_slot = Some(StatusSlot::Spend);
    spend.used_money = Some(Money::new(27_00, "USD", 2));
    spend.limit_money = Some(Money::new(30_000, "USD", 2));
    spend.used_label = Some("$27.00 of $300.00".into());
    spend.resets_at = Some(1_800_200_000);
    let entry = catalog_entry(
        view_with_buckets(UsageSnapshotStatus::Fresh, vec![weekly, spend]),
        Some("Pro"),
    );

    let account = project_account(&entry, 0, 7).unwrap();
    // Principal-window projection keeps its shape: two buckets, two windows.
    assert_eq!(account.windows.len(), 2);
    assert_eq!(
        account.windows[0].remaining_percent.map(UsagePercent::get),
        Some(57)
    );
    assert_eq!(account.windows[0].remaining_raw_percent, Some(57));

    let kinds = account
        .metric_groups
        .iter()
        .map(|group| group.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            UsageMetricGroupKindV1::Window,
            UsageMetricGroupKindV1::Window,
            UsageMetricGroupKindV1::SpendCap,
            UsageMetricGroupKindV1::Plan,
        ]
    );
    for (rank, group) in account.metric_groups.iter().enumerate() {
        group.validate(rank).unwrap();
        assert_eq!(group.fetched_at_epoch, 1_800_000_000);
        assert_eq!(group.observed_at_epoch, Some(1_800_000_000));
        assert_eq!(group.last_success_at_epoch, Some(1_800_000_000));
        assert!(!group.is_stale);
    }
    // Window group mirrors its bucket window.
    assert_eq!(
        account.metric_groups[0].quota_state,
        account.windows[0].quota_state
    );
    assert_eq!(account.metric_groups[0].reset_at_epoch, Some(1_800_100_000));
    assert_eq!(account.metric_groups[0].renews_at_epoch, None);
    // Spend group carries structured money with currency and exponent.
    match &account.metric_groups[2].value {
        UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } => {
            assert_eq!(cap, &Some(Money::new(30_000, "USD", 2)));
            assert_eq!(spent, &Some(Money::new(27_00, "USD", 2)));
            assert_eq!(remaining, &Some(Money::new(30_000 - 27_00, "USD", 2)));
        }
        other => panic!("expected spend-cap value, got {other:?}"),
    }
    assert_eq!(account.metric_groups[2].reset_at_epoch, Some(1_800_200_000));
    // Plan group carries metadata with no quota notion and no reset.
    assert_eq!(
        account.metric_groups[3].quota_state,
        UsageQuotaStateV1::NotApplicable
    );
    assert_eq!(account.metric_groups[3].reset_at_epoch, None);
    assert_eq!(account.metric_groups[3].renews_at_epoch, None);

    // Group ids are stable for identical input.
    let rerun = project_account(&entry, 0, 7).unwrap();
    let ids = account
        .metric_groups
        .iter()
        .map(|group| group.group_id.clone())
        .collect::<Vec<_>>();
    let rerun_ids = rerun
        .metric_groups
        .iter()
        .map(|group| group.group_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(ids, rerun_ids);
}

#[test]
fn spend_group_states_cover_ratio_edges() {
    let mut over = bucket("Over");
    over.used_money = Some(Money::new(12_000, "USD", 2));
    over.limit_money = Some(Money::new(10_000, "USD", 2));
    assert_eq!(spend_quota_state(&over), UsageQuotaStateV1::Exhausted);

    let mut warn = bucket("Warn");
    warn.used_money = Some(Money::new(80_00, "USD", 2));
    warn.limit_money = Some(Money::new(10_000, "USD", 2));
    assert_eq!(spend_quota_state(&warn), UsageQuotaStateV1::Warning);

    let mut ok = bucket("Ok");
    ok.used_money = Some(Money::new(10_00, "USD", 2));
    ok.limit_money = Some(Money::new(10_000, "USD", 2));
    assert_eq!(spend_quota_state(&ok), UsageQuotaStateV1::Available);

    let mut uncapped = bucket("Uncapped");
    uncapped.used_money = Some(Money::new(10_00, "USD", 2));
    assert_eq!(
        spend_quota_state(&uncapped),
        UsageQuotaStateV1::NotApplicable
    );

    let mut cap_only = bucket("Cap only");
    cap_only.limit_money = Some(Money::new(10_000, "USD", 2));
    assert_eq!(spend_quota_state(&cap_only), UsageQuotaStateV1::Unknown);

    let mut mismatched = bucket("Mismatched");
    mismatched.used_money = Some(Money::new(10_00, "USD", 2));
    mismatched.limit_money = Some(Money::new(10_000, "SGD", 2));
    assert_eq!(spend_quota_state(&mismatched), UsageQuotaStateV1::Unknown);

    let mut login = bucket("Login");
    login.status = UsageSnapshotStatus::NeedsLogin;
    login.used_money = Some(Money::new(10_00, "USD", 2));
    login.limit_money = Some(Money::new(10_000, "USD", 2));
    assert_eq!(spend_quota_state(&login), UsageQuotaStateV1::NoPermission);
}

fn status_bucket(label: &str, status: UsageSnapshotStatus) -> QuotaBucketView {
    let mut bucket = bucket(label);
    bucket.remaining_percent = Some(57);
    bucket.status = status;
    bucket
}

#[test]
fn group_timestamps_track_view_usability() {
    let fresh = catalog_entry(
        view_with_buckets(
            UsageSnapshotStatus::Fresh,
            vec![status_bucket("Weekly", UsageSnapshotStatus::Fresh)],
        ),
        None,
    );
    let account = project_account(&fresh, 0, 1).unwrap();
    assert_eq!(account.metric_groups.len(), 1);
    assert_eq!(
        account.metric_groups[0].phase,
        UsageFreshnessPhaseV1::Current
    );
    assert_eq!(
        account.metric_groups[0].last_success_at_epoch,
        Some(1_800_000_000)
    );

    let stale = catalog_entry(
        view_with_buckets(
            UsageSnapshotStatus::Stale,
            vec![status_bucket("Weekly", UsageSnapshotStatus::Stale)],
        ),
        None,
    );
    let account = project_account(&stale, 0, 1).unwrap();
    assert_eq!(account.metric_groups[0].phase, UsageFreshnessPhaseV1::Stale);
    assert!(account.metric_groups[0].is_stale);
    assert_eq!(
        account.metric_groups[0].last_success_at_epoch,
        Some(1_800_000_000)
    );

    let error = catalog_entry(
        view_with_buckets(
            UsageSnapshotStatus::Error,
            vec![status_bucket("Weekly", UsageSnapshotStatus::Error)],
        ),
        None,
    );
    let account = project_account(&error, 0, 1).unwrap();
    assert_eq!(
        account.metric_groups[0].phase,
        UsageFreshnessPhaseV1::Failed
    );
    assert_eq!(account.metric_groups[0].last_success_at_epoch, None);
    assert_eq!(
        account.metric_groups[0].quota_state,
        UsageQuotaStateV1::Error
    );
}

#[test]
fn credential_expiry_stays_unset_without_provider_signal() {
    let mut weekly = bucket("Weekly");
    weekly.remaining_percent = Some(57);
    weekly.resets_at = Some(1_800_100_000);
    let entry = catalog_entry(
        view_with_buckets(UsageSnapshotStatus::Fresh, vec![weekly]),
        None,
    );
    let account = project_account(&entry, 0, 1).unwrap();
    assert_eq!(account.windows[0].reset_at_epoch, Some(1_800_100_000));
    assert_eq!(account.credential_expires_at_epoch, None);
}

#[test]
fn canonical_projection_icu_collation_goldens_are_pinned() {
    assert_eq!(
        sorted("und", &["Zulu", "Änne", "Ana", "Åke"]),
        ["Åke", "Ana", "Änne", "Zulu"]
    );
    assert_eq!(
        sorted("en", &["Zulu", "Änne", "Ana", "Åke"]),
        ["Åke", "Ana", "Änne", "Zulu"]
    );
    assert_eq!(
        sorted("tr", &["Jale", "İpek", "Işık", "Hale"]),
        ["Hale", "Işık", "İpek", "Jale"]
    );
    assert_eq!(
        sorted("vi", &["Bình", "Ân", "Ăn", "An"]),
        ["An", "Ăn", "Ân", "Bình"]
    );
}

// ── S4/S5 Console↔Capsule parity harness ────────────────────────────────
//
// ONE fixture scenario feeds both renderers under a fixed clock, proving they
// show the same canonical usage meaning:
//
// - Console: fixture `FocusedUsageView`s → the REAL `project_account` → a
//   validated `UsageProjectionV1` → the real
//   `UsageScreenState::from_projection` → the renderer's public label, meter,
//   and freshness helpers (plus one `TestBackend` render smoke test for the
//   renderer-private strings).
// - Capsule: the SAME views → the real `provider_tabs` /
//   `enrich_provider_tabs` / `mark_active_tab` / `usage_detail_presentation` /
//   `usage_identity_presentation` / `usage_bucket_presentation` paths.
//
// The only hand-built projection parts are broker-owned overlays (issues,
// credential expiry, unresolved entries, projection issues) and typed groups
// no producer emits yet (balance / rate-limit / token-totals / scoped
// groups); provider assembly reuses the real `provider_freshness`, and every
// assembled projection passes the real `UsageProjectionV1::validate`.
// No provider I/O anywhere: all views are fixture-built, the clock is fixed,
// and timezone-dependent fragments are asserted by prefix only.
//
// Tests named `parity_*` assert cross-surface agreement. Tests named
// `documented_delta_*` lock a KNOWN divergence with its disposition; see the
// S4/S5 parity report for the full delta list with file:line evidence.
use std::collections::HashMap;
use std::collections::HashSet;

use jackin_protocol::control::{UsageActivityKind, UsageDetailRowKind};
use jackin_protocol::usage_broker::{UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1};

use crate::host::HostSurfaceId;
use crate::usage::{
    CachedUsage, UsageSurface, UsageViewInput, enrich_provider_tabs, mark_active_tab,
    provider_display_label, provider_tabs, refresh_cached_updated_label, timed_bucket,
    usage_bucket_presentation, usage_detail_presentation, usage_identity_presentation, usage_view,
};

/// Fixed harness clock (UTC epoch seconds). Fixture ages and resets derive
/// from this; nothing reads wall-clock time.
const PARITY_NOW: i64 = 1_800_000_000;

/// Harness broker generation threaded through the real `project_account`.
const PARITY_GENERATION: u64 = 7;

/// One quota bucket shared by both surfaces: the capsule view is built with
/// the real `timed_bucket`, the console window with the real
/// `project_account`.
#[derive(Debug, Clone)]
struct ParityBucket {
    label: &'static str,
    slot: Option<StatusSlot>,
    used_label: Option<&'static str>,
    limit_label: Option<&'static str>,
    remaining: Option<u8>,
    reset_at: Option<i64>,
    pace: Option<&'static str>,
    status: UsageSnapshotStatus,
    severity: UsageSeverity,
    used_money: Option<Money>,
    limit_money: Option<Money>,
}

impl ParityBucket {
    /// Minimal fresh bucket with a remaining percent and no reset/money.
    fn metered(label: &'static str, remaining: u8) -> Self {
        Self {
            label,
            slot: None,
            used_label: None,
            limit_label: None,
            remaining: Some(remaining),
            reset_at: None,
            pace: None,
            status: UsageSnapshotStatus::Fresh,
            severity: UsageSeverity::Normal,
            used_money: None,
            limit_money: None,
        }
    }
}

/// One account shared by both surfaces.
#[derive(Debug, Clone)]
struct ParityAccount {
    surface: HostSurfaceId,
    provider_id: &'static str,
    provider_label: &'static str,
    subject: CanonicalAccountSubject,
    account_key: &'static str,
    account_label: &'static str,
    username: Option<&'static str>,
    plan_label: Option<&'static str>,
    credential_origin: Option<&'static str>,
    /// Capsule provider label override for surfaces without a `UsageSurface`
    /// variant (Cursor, Antigravity); mirrors the adapter pattern of building
    /// with `Unsupported` and then stamping the label.
    capsule_provider_label: Option<&'static str>,
    agent: &'static str,
    focused_provider: Option<&'static str>,
    status: UsageSnapshotStatus,
    source: UsageSource,
    confidence: UsageConfidence,
    fetched_at: i64,
    buckets: Vec<ParityBucket>,
    last_error: Option<&'static str>,
    /// Broker-owned overlays (populated post-projection, as the broker does).
    account_issues: Vec<UsageIssueV1>,
    credential_expires_at: Option<i64>,
    extra_groups: Vec<ParityExtraGroup>,
}

/// Typed metric group no producer emits yet (balance / rate-limit /
/// token-totals / scoped groups). Rank and id are assigned at assembly.
#[derive(Debug, Clone)]
struct ParityExtraGroup {
    kind: UsageMetricGroupKindV1,
    label: &'static str,
    scope: UsageMetricScopeV1,
    value: UsageMetricValueV1,
    quota_state: UsageQuotaStateV1,
    phase: UsageFreshnessPhaseV1,
    is_stale: bool,
    observed_at: Option<i64>,
    fetched_at: i64,
    last_success_at: Option<i64>,
    reset_at: Option<i64>,
    renews_at: Option<i64>,
}

fn parity_issue(
    code: &'static str,
    scope: UsageIssueScopeV1,
    recoverability: UsageIssueRecoverabilityV1,
    message: &'static str,
    retry_at: Option<i64>,
) -> UsageIssueV1 {
    UsageIssueV1 {
        code: code.to_owned(),
        scope,
        recoverability,
        message: message.to_owned(),
        retry_at_epoch: retry_at,
    }
}

/// Clock-parameterized bucket builder. Structural tests pass the fixed
/// `PARITY_NOW`; render tests pass wall time because the console renderer
/// hardcodes a wall-clock `now` for its relative labels.
fn parity_bucket_view_at(def: &ParityBucket, now: i64) -> QuotaBucketView {
    let mut view = timed_bucket(
        def.label,
        def.used_label.map(str::to_owned),
        def.limit_label.map(str::to_owned),
        def.remaining,
        def.reset_at,
        now,
        def.pace,
        def.status,
    );
    view.status_slot = def.slot;
    view.severity = def.severity;
    view.used_money = def.used_money.clone();
    view.limit_money = def.limit_money.clone();
    view
}

/// Real `UsageSurface` for providers that have one; `Unsupported` otherwise
/// (the adapter then stamps the capsule label explicitly).
fn parity_surface(provider_id: &str) -> UsageSurface {
    match provider_id {
        "anthropic" => UsageSurface::Claude,
        "openai" => UsageSurface::Codex,
        "xai" => UsageSurface::Grok,
        "zai" => UsageSurface::Zai,
        "kimi" => UsageSurface::Kimi,
        "minimax" => UsageSurface::Minimax,
        "opencode" => UsageSurface::OpenCode,
        _ => UsageSurface::Unsupported,
    }
}

/// Capsule-side canonical snapshot via the real `usage_view` constructor,
/// with the adapter-pattern label override for surfaceless providers.
fn parity_view(account: &ParityAccount) -> FocusedUsageView {
    parity_view_at(account, PARITY_NOW)
}

fn parity_view_at(account: &ParityAccount, now: i64) -> FocusedUsageView {
    let buckets = account
        .buckets
        .iter()
        .map(|def| parity_bucket_view_at(def, now))
        .collect::<Vec<_>>();
    let mut view = usage_view(UsageViewInput {
        agent: account.agent,
        provider: account.focused_provider,
        surface: parity_surface(account.provider_id),
        account_label: account.account_label.to_owned(),
        username: account.username.map(str::to_owned),
        plan_label: account.plan_label.map(str::to_owned),
        credential_origin: account.credential_origin.map(str::to_owned),
        buckets,
        status: account.status,
        source: account.source,
        confidence: account.confidence,
        now: account.fetched_at,
        last_error: account.last_error.map(str::to_owned),
    });
    if let Some(label) = account.capsule_provider_label {
        view.account.provider_label = label.to_owned();
    }
    // Mirror the cache read path at the harness clock so `updated_label`
    // carries the deterministic fixture age instead of build time.
    refresh_cached_updated_label(&mut view, now);
    view
}

fn parity_entry(account: &ParityAccount, view: FocusedUsageView) -> AccountCatalogEntry {
    AccountCatalogEntry {
        identity: CanonicalAccountIdentity {
            surface: account.surface,
            subject: account.subject.clone(),
        },
        account_key: account.account_key.to_owned(),
        account_label: account.account_label.to_owned(),
        username: account.username.map(str::to_owned),
        plan_label: account.plan_label.map(str::to_owned),
        provenance: BTreeSet::from([AccountProvenance::LiveHost]),
        discovery_provenance: BTreeSet::from(["parity-fixture".to_owned()]),
        lifecycle: AccountLifecycle::Current,
        view,
        fetched_at_epoch: account.fetched_at,
    }
}

fn parity_extra_group(canonical_id: &str, rank: u32, def: &ParityExtraGroup) -> UsageMetricGroupV1 {
    UsageMetricGroupV1 {
        group_id: format!("parity-extra:{canonical_id}:{rank}"),
        rank,
        kind: def.kind,
        label: def.label.to_owned(),
        scope: def.scope.clone(),
        observed_at_epoch: def.observed_at,
        fetched_at_epoch: def.fetched_at,
        last_success_at_epoch: def.last_success_at,
        phase: def.phase,
        is_stale: def.is_stale,
        quota_state: def.quota_state,
        value: def.value.clone(),
        reset_at_epoch: def.reset_at,
        renews_at_epoch: def.renews_at,
        issues: Vec::new(),
    }
}

/// Console-side canonical account: the REAL `project_account` over the
/// fixture view, plus broker-owned overlays and hand-built extra groups.
fn parity_project_account(
    account: &ParityAccount,
    view: &FocusedUsageView,
    rank: usize,
) -> UsageAccountV1 {
    let entry = parity_entry(account, view.clone());
    let mut projected = project_account(&entry, rank, PARITY_GENERATION).unwrap();
    for def in &account.extra_groups {
        let group_rank = u32::try_from(projected.metric_groups.len()).unwrap();
        projected.metric_groups.push(parity_extra_group(
            &projected.canonical_account_id,
            group_rank,
            def,
        ));
    }
    projected.issues = account.account_issues.clone();
    projected.credential_expires_at_epoch = account.credential_expires_at;
    projected
}

/// One provider group in the assembled projection.
struct ParityProvider {
    provider_id: &'static str,
    display_name: &'static str,
    accounts: Vec<ParityAccount>,
    provider_issues: Vec<UsageIssueV1>,
}

/// Assemble and validate a canonical projection from fixture providers.
/// Provider assembly reuses the real `provider_freshness`.
fn parity_projection(
    providers: &[ParityProvider],
    unresolved: Vec<UsageUnresolvedV1>,
    projection_issues: Vec<UsageIssueV1>,
) -> (UsageProjectionV1, Vec<Vec<FocusedUsageView>>) {
    parity_projection_at(PARITY_NOW, providers, unresolved, projection_issues)
}

fn parity_projection_at(
    now: i64,
    providers: &[ParityProvider],
    unresolved: Vec<UsageUnresolvedV1>,
    projection_issues: Vec<UsageIssueV1>,
) -> (UsageProjectionV1, Vec<Vec<FocusedUsageView>>) {
    let mut views_by_provider = Vec::new();
    let mut projected = Vec::new();
    for (provider_rank, provider) in providers.iter().enumerate() {
        let mut views = Vec::new();
        let mut accounts = Vec::new();
        for (account_rank, account) in provider.accounts.iter().enumerate() {
            debug_assert_eq!(
                provider.display_name, account.provider_label,
                "fixture provider label must match its group"
            );
            let view = parity_view_at(account, now);
            views.push(view.clone());
            accounts.push(parity_project_account(account, &view, account_rank));
        }
        views_by_provider.push(views);
        let freshness = provider_freshness(&accounts, PARITY_GENERATION);
        projected.push(UsageProviderV1 {
            provider_id: provider.provider_id.to_owned(),
            display_name: provider.display_name.to_owned(),
            rank: u32::try_from(provider_rank).unwrap(),
            membership_state: UsageMembershipStateV1::Current,
            freshness,
            accounts,
            issues: provider.provider_issues.clone(),
        });
    }
    let projection = UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "parity-fixture:1".to_owned(),
        generated_at_epoch: now,
        discovery_revision: "parity".to_owned(),
        broker_instance_id: "parity-broker".to_owned(),
        broker_generation: PARITY_GENERATION,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: projected,
        unresolved,
        issues: projection_issues,
    };
    projection.validate().unwrap();
    (projection, views_by_provider)
}

/// Capsule tab strip over fixture views via the real `provider_tabs`, with
/// `enrich` + active marking applied the way the cache read path does.
fn parity_tabs(views: &[FocusedUsageView]) -> Vec<FocusedUsageView> {
    let snapshots: HashMap<String, CachedUsage> = views
        .iter()
        .enumerate()
        .map(|(index, view)| {
            (
                format!("parity-snapshot:{index}"),
                CachedUsage { view: view.clone() },
            )
        })
        .collect();
    views
        .iter()
        .map(|view| {
            let mut enriched = view.clone();
            enrich_provider_tabs(&mut enriched, &snapshots);
            mark_active_tab(&mut enriched);
            enriched
        })
        .collect()
}

fn usd(cents: i64) -> Money {
    Money::new(cents, "USD", 2)
}

/// Antigravity-style two-family fixture: Gemini pools plus Claude/GPT
/// ("Other models") pools with distinct five-hour and weekly values.
fn parity_antigravity_account() -> ParityAccount {
    ParityAccount {
        // No `HostSurfaceId::Antigravity` exists; the surface only feeds the
        // opaque canonical-id hash here (Antigravity is Google tooling).
        surface: HostSurfaceId::Google,
        provider_id: "antigravity",
        provider_label: "Antigravity",
        subject: CanonicalAccountSubject::ProviderStableHandle(
            "parity-antigravity-pilot".to_owned(),
        ),
        account_key: "antigravity:pilot",
        account_label: "pilot@example.test",
        username: None,
        plan_label: Some("Antigravity Pro"),
        credential_origin: Some("CLI · agy"),
        capsule_provider_label: Some("Antigravity"),
        agent: "codex",
        focused_provider: Some("Antigravity"),
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::Cli,
        confidence: UsageConfidence::Authoritative,
        fetched_at: PARITY_NOW - 300,
        buckets: vec![
            ParityBucket {
                slot: Some(StatusSlot::Session),
                used_label: Some("27% used"),
                limit_label: Some("100%"),
                reset_at: Some(PARITY_NOW + 5_430),
                pace: Some("On pace"),
                ..ParityBucket::metered("Gemini · 5h", 73)
            },
            ParityBucket {
                slot: Some(StatusSlot::Weekly),
                used_label: Some("59% used"),
                limit_label: Some("100%"),
                reset_at: Some(PARITY_NOW + 90_000),
                pace: Some("13% in reserve"),
                ..ParityBucket::metered("Gemini · Weekly", 41)
            },
            ParityBucket {
                used_label: Some("88% used"),
                limit_label: Some("100%"),
                reset_at: Some(PARITY_NOW + 5_430),
                pace: Some("5% in deficit"),
                ..ParityBucket::metered("Other models · 5h", 12)
            },
            ParityBucket {
                used_label: Some("12% used"),
                limit_label: Some("100%"),
                reset_at: Some(PARITY_NOW + 90_000),
                pace: Some("On pace"),
                ..ParityBucket::metered("Other models · Weekly", 88)
            },
        ],
        last_error: None,
        account_issues: Vec::new(),
        credential_expires_at: None,
        extra_groups: vec![ParityExtraGroup {
            kind: UsageMetricGroupKindV1::Balance,
            label: "Credits",
            scope: UsageMetricScopeV1 {
                service: None,
                model: None,
                pool: Some("credits-pool".to_owned()),
                key_id: None,
            },
            value: UsageMetricValueV1::Balance {
                amount: usd(1_250),
                expires_at_epoch: Some(PARITY_NOW + 2_592_000),
            },
            // No producer rule assigns balance quota states yet; the balance
            // carries a usable quantity, so `Available` (harness choice).
            quota_state: UsageQuotaStateV1::Available,
            phase: UsageFreshnessPhaseV1::Current,
            is_stale: false,
            observed_at: Some(PARITY_NOW - 300),
            fetched_at: PARITY_NOW - 300,
            last_success_at: Some(PARITY_NOW - 300),
            reset_at: None,
            renews_at: None,
        }],
    }
}

fn parity_claude_work_account() -> ParityAccount {
    ParityAccount {
        surface: HostSurfaceId::Claude,
        provider_id: "anthropic",
        provider_label: "Anthropic",
        subject: CanonicalAccountSubject::ProviderId("anthropic-acct-work".to_owned()),
        account_key: "claude:work",
        account_label: "work@example.test",
        username: Some("work-user"),
        plan_label: Some("Max 20x"),
        credential_origin: Some("OAuth · keychain"),
        capsule_provider_label: None,
        agent: "claude",
        focused_provider: Some("Anthropic"),
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at: PARITY_NOW - 120,
        buckets: vec![
            ParityBucket {
                slot: Some(StatusSlot::Session),
                used_label: Some("11% used"),
                limit_label: Some("100%"),
                reset_at: Some(PARITY_NOW + 3_600),
                pace: Some("On pace"),
                ..ParityBucket::metered("Session", 89)
            },
            ParityBucket {
                slot: Some(StatusSlot::Weekly),
                used_label: Some("27% used"),
                limit_label: Some("100%"),
                reset_at: Some(PARITY_NOW + 80_000),
                ..ParityBucket::metered("Weekly", 73)
            },
        ],
        last_error: None,
        account_issues: Vec::new(),
        credential_expires_at: None,
        extra_groups: vec![ParityExtraGroup {
            kind: UsageMetricGroupKindV1::TokenTotals,
            label: "Tokens",
            scope: UsageMetricScopeV1 {
                service: None,
                model: Some("claude-opus-4-6".to_owned()),
                pool: None,
                key_id: None,
            },
            value: UsageMetricValueV1::TokenTotals {
                input: Some(1_500_000),
                output: Some(320_000),
                cached: Some(900_000),
                reasoning: None,
                interval_label: Some("this week".to_owned()),
            },
            quota_state: UsageQuotaStateV1::NotApplicable,
            phase: UsageFreshnessPhaseV1::Current,
            is_stale: false,
            observed_at: Some(PARITY_NOW - 120),
            fetched_at: PARITY_NOW - 120,
            last_success_at: Some(PARITY_NOW - 120),
            reset_at: None,
            renews_at: None,
        }],
    }
}

fn parity_claude_personal_account() -> ParityAccount {
    ParityAccount {
        surface: HostSurfaceId::Claude,
        provider_id: "anthropic",
        provider_label: "Anthropic",
        subject: CanonicalAccountSubject::ProviderStableHandle("personal@example.test".to_owned()),
        account_key: "claude:personal",
        account_label: "personal@example.test",
        username: None,
        plan_label: Some("Max"),
        credential_origin: Some("OAuth · keychain"),
        capsule_provider_label: None,
        agent: "claude",
        focused_provider: Some("Anthropic"),
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at: PARITY_NOW - 120,
        buckets: vec![
            ParityBucket {
                slot: Some(StatusSlot::Session),
                used_label: Some("55% used"),
                limit_label: Some("100%"),
                reset_at: Some(PARITY_NOW + 3_600),
                ..ParityBucket::metered("Session", 45)
            },
            ParityBucket {
                slot: Some(StatusSlot::Weekly),
                used_label: Some("92% used"),
                limit_label: Some("100%"),
                reset_at: Some(PARITY_NOW + 80_000),
                severity: UsageSeverity::Danger,
                ..ParityBucket::metered("Weekly", 8)
            },
            // Overdrawn spend: $150 against a $100 cap (raw 150% used).
            ParityBucket {
                label: "Extra usage",
                slot: Some(StatusSlot::Spend),
                used_label: Some("$150.00 spent"),
                limit_label: Some("$100.00"),
                remaining: Some(0),
                reset_at: None,
                pace: Some("150% used"),
                status: UsageSnapshotStatus::Fresh,
                severity: UsageSeverity::Danger,
                used_money: Some(usd(15_000)),
                limit_money: Some(usd(10_000)),
            },
        ],
        last_error: None,
        account_issues: Vec::new(),
        credential_expires_at: None,
        extra_groups: Vec::new(),
    }
}

/// Cursor groups: billing-cycle meter, actual spend, credit balance (used-only
/// money, mirroring `cursor_credits_bucket`), and request counts.
fn parity_cursor_account() -> ParityAccount {
    ParityAccount {
        surface: HostSurfaceId::Cursor,
        provider_id: "cursor",
        provider_label: "Cursor",
        subject: CanonicalAccountSubject::ProviderStableHandle("cursor-user".to_owned()),
        account_key: "cursor:user",
        account_label: "cursor-user",
        username: None,
        plan_label: Some("Pro"),
        credential_origin: Some("OAuth · cursor.com"),
        capsule_provider_label: Some("Cursor"),
        agent: "codex",
        focused_provider: Some("Cursor"),
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at: PARITY_NOW - 600,
        buckets: vec![
            ParityBucket {
                slot: Some(StatusSlot::Weekly),
                used_label: Some("38% used"),
                limit_label: Some("100%"),
                reset_at: Some(PARITY_NOW + 1_200_000),
                severity: UsageSeverity::Warn,
                ..ParityBucket::metered("Billing cycle", 62)
            },
            ParityBucket {
                label: "Spend (actual)",
                slot: Some(StatusSlot::Spend),
                used_label: Some("$45.20 spent"),
                limit_label: Some("$100.00"),
                remaining: Some(55),
                reset_at: None,
                pace: None,
                status: UsageSnapshotStatus::Fresh,
                severity: UsageSeverity::Normal,
                used_money: Some(usd(4_520)),
                limit_money: Some(usd(10_000)),
            },
            ParityBucket {
                label: "Credits",
                slot: None,
                used_label: Some("$8.30"),
                limit_label: None,
                remaining: None,
                reset_at: None,
                pace: None,
                status: UsageSnapshotStatus::Fresh,
                severity: UsageSeverity::Normal,
                used_money: Some(usd(830)),
                limit_money: None,
            },
            ParityBucket {
                label: "Requests",
                slot: None,
                used_label: Some("1.2K"),
                limit_label: Some("5.0K"),
                remaining: Some(76),
                reset_at: None,
                pace: None,
                status: UsageSnapshotStatus::Fresh,
                severity: UsageSeverity::Normal,
                used_money: None,
                limit_money: None,
            },
        ],
        last_error: None,
        account_issues: Vec::new(),
        credential_expires_at: None,
        extra_groups: vec![ParityExtraGroup {
            kind: UsageMetricGroupKindV1::RateLimit,
            label: "API rate limit",
            scope: UsageMetricScopeV1::default(),
            value: UsageMetricValueV1::RateLimit {
                limit: Some(100),
                remaining: Some(20),
                window_label: Some("per minute".to_owned()),
            },
            quota_state: UsageQuotaStateV1::Available,
            phase: UsageFreshnessPhaseV1::Current,
            is_stale: false,
            observed_at: Some(PARITY_NOW - 600),
            fetched_at: PARITY_NOW - 600,
            last_success_at: Some(PARITY_NOW - 600),
            reset_at: Some(PARITY_NOW + 90),
            renews_at: None,
        }],
    }
}

/// Legit-zero exhaustion: 0% left from an authoritative response, plus a
/// missing plan label.
fn parity_exhausted_account() -> ParityAccount {
    ParityAccount {
        surface: HostSurfaceId::Codex,
        provider_id: "openai",
        provider_label: "OpenAI",
        subject: CanonicalAccountSubject::ProviderStableHandle("zero@example.test".to_owned()),
        account_key: "openai:zero",
        account_label: "zero@example.test",
        username: None,
        plan_label: None,
        credential_origin: Some("OAuth · keychain"),
        capsule_provider_label: None,
        agent: "codex",
        focused_provider: Some("OpenAI"),
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at: PARITY_NOW - 60,
        buckets: vec![ParityBucket {
            slot: Some(StatusSlot::Session),
            used_label: Some("100% used"),
            limit_label: Some("100%"),
            reset_at: Some(PARITY_NOW + 1_800),
            pace: Some("100% in deficit"),
            ..ParityBucket::metered("Session", 0)
        }],
        last_error: None,
        account_issues: Vec::new(),
        credential_expires_at: None,
        extra_groups: Vec::new(),
    }
}

/// Stale last-good with a partial window set: one metered window, one
/// limit-only balance (mirroring the Grok prepaid seam), one quantity-less
/// unknown window, and a rate-limit issue carrying the broker retry.
fn parity_partial_account() -> ParityAccount {
    ParityAccount {
        surface: HostSurfaceId::Grok,
        provider_id: "xai",
        provider_label: "xAI",
        subject: CanonicalAccountSubject::ProviderStableHandle("partial@example.test".to_owned()),
        account_key: "grok:partial",
        account_label: "partial@example.test",
        username: None,
        plan_label: Some("SuperGrok"),
        credential_origin: Some("API key · env"),
        capsule_provider_label: None,
        agent: "grok",
        focused_provider: Some("xAI"),
        status: UsageSnapshotStatus::Stale,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at: PARITY_NOW - 1_500,
        buckets: vec![
            ParityBucket {
                slot: Some(StatusSlot::Weekly),
                used_label: Some("46% used"),
                limit_label: Some("100%"),
                reset_at: Some(PARITY_NOW + 70_000),
                status: UsageSnapshotStatus::Stale,
                ..ParityBucket::metered("Weekly", 54)
            },
            ParityBucket {
                label: "Extra usage credits",
                slot: None,
                used_label: None,
                limit_label: Some("$5.00"),
                remaining: None,
                reset_at: None,
                pace: None,
                status: UsageSnapshotStatus::Stale,
                severity: UsageSeverity::Normal,
                used_money: None,
                limit_money: Some(usd(500)),
            },
            ParityBucket {
                label: "MCP",
                slot: None,
                used_label: None,
                limit_label: None,
                remaining: None,
                reset_at: None,
                pace: None,
                status: UsageSnapshotStatus::Stale,
                severity: UsageSeverity::Normal,
                used_money: None,
                limit_money: None,
            },
        ],
        last_error: Some("rate limited by provider; showing last cached quota"),
        account_issues: vec![parity_issue(
            "rate_limited",
            UsageIssueScopeV1::Account,
            UsageIssueRecoverabilityV1::Retryable,
            "rate limited by provider",
            Some(PARITY_NOW + 330),
        )],
        credential_expires_at: None,
        extra_groups: Vec::new(),
    }
}

fn parity_unsupported_account() -> ParityAccount {
    ParityAccount {
        surface: HostSurfaceId::Minimax,
        provider_id: "minimax",
        provider_label: "MiniMax",
        subject: CanonicalAccountSubject::ProviderStableHandle("mm-user".to_owned()),
        account_key: "minimax:user",
        account_label: "mm-user",
        username: None,
        plan_label: None,
        credential_origin: None,
        capsule_provider_label: None,
        agent: "codex",
        focused_provider: Some("MiniMax"),
        status: UsageSnapshotStatus::Unsupported,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        fetched_at: PARITY_NOW - 60,
        buckets: vec![ParityBucket {
            label: "Quota",
            slot: None,
            used_label: None,
            limit_label: None,
            remaining: None,
            reset_at: None,
            pace: None,
            status: UsageSnapshotStatus::Unsupported,
            severity: UsageSeverity::Normal,
            used_money: None,
            limit_money: None,
        }],
        last_error: Some("usage limits unsupported"),
        account_issues: Vec::new(),
        credential_expires_at: None,
        extra_groups: Vec::new(),
    }
}

/// Auth-expired login state: credential expiry in the past, no windows.
fn parity_auth_account() -> ParityAccount {
    ParityAccount {
        surface: HostSurfaceId::Zai,
        provider_id: "zai",
        provider_label: "Z.AI",
        subject: CanonicalAccountSubject::SourceCapability("zai:default".to_owned()),
        account_key: "zai:default",
        account_label: "zai-user",
        username: None,
        plan_label: None,
        credential_origin: None,
        capsule_provider_label: None,
        agent: "codex",
        focused_provider: Some("Z.AI"),
        status: UsageSnapshotStatus::NeedsLogin,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        fetched_at: PARITY_NOW - 60,
        buckets: Vec::new(),
        last_error: Some("sign in required"),
        account_issues: vec![parity_issue(
            "auth_required",
            UsageIssueScopeV1::Account,
            UsageIssueRecoverabilityV1::ActionRequired,
            "sign in required",
            None,
        )],
        credential_expires_at: Some(PARITY_NOW - 3_600),
        extra_groups: Vec::new(),
    }
}

/// Hard failure: timeout plus malformed issues, no windows.
fn parity_error_account() -> ParityAccount {
    ParityAccount {
        surface: HostSurfaceId::Kimi,
        provider_id: "kimi",
        provider_label: "Kimi",
        subject: CanonicalAccountSubject::ProviderStableHandle("kimi-user".to_owned()),
        account_key: "kimi:user",
        account_label: "kimi-user",
        username: None,
        plan_label: None,
        credential_origin: Some("API key · env"),
        capsule_provider_label: None,
        agent: "kimi",
        focused_provider: Some("Kimi"),
        status: UsageSnapshotStatus::Error,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        fetched_at: PARITY_NOW - 90,
        buckets: Vec::new(),
        last_error: Some("usage request timed out"),
        account_issues: vec![
            parity_issue(
                "timeout",
                UsageIssueScopeV1::Account,
                UsageIssueRecoverabilityV1::Retryable,
                "usage request timed out",
                Some(PARITY_NOW + 150),
            ),
            parity_issue(
                "malformed",
                UsageIssueScopeV1::Account,
                UsageIssueRecoverabilityV1::Terminal,
                "usage response malformed",
                None,
            ),
        ],
        credential_expires_at: None,
        extra_groups: Vec::new(),
    }
}

/// Legit-zero spend ($0 of $100 tracked, not missing) with missing identity
/// fields: empty account label, no username/plan/origin, no reset.
fn parity_zero_account() -> ParityAccount {
    ParityAccount {
        surface: HostSurfaceId::OpenCode,
        provider_id: "opencode",
        provider_label: "OpenCode",
        subject: CanonicalAccountSubject::SourceCapability("opencode:default".to_owned()),
        account_key: "opencode:default",
        account_label: "",
        username: None,
        plan_label: None,
        credential_origin: None,
        capsule_provider_label: None,
        agent: "opencode",
        focused_provider: Some("OpenCode"),
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::LocalLogs,
        confidence: UsageConfidence::Estimated,
        fetched_at: PARITY_NOW - 30,
        buckets: vec![ParityBucket {
            label: "Tokens",
            slot: Some(StatusSlot::Spend),
            used_label: Some("$0.00"),
            limit_label: Some("$100.00"),
            remaining: Some(100),
            reset_at: None,
            pace: None,
            status: UsageSnapshotStatus::Fresh,
            severity: UsageSeverity::Normal,
            used_money: Some(usd(0)),
            limit_money: Some(usd(10_000)),
        }],
        last_error: None,
        account_issues: Vec::new(),
        credential_expires_at: None,
        extra_groups: Vec::new(),
    }
}

/// Day-old stale account for S5 (fresh sibling must not clear its staleness).
fn parity_old_stale_account() -> ParityAccount {
    ParityAccount {
        surface: HostSurfaceId::Claude,
        provider_id: "anthropic",
        provider_label: "Anthropic",
        subject: CanonicalAccountSubject::ProviderStableHandle("old@example.test".to_owned()),
        account_key: "claude:old",
        account_label: "old@example.test",
        username: None,
        plan_label: Some("Max"),
        credential_origin: Some("OAuth · keychain"),
        capsule_provider_label: None,
        agent: "claude",
        focused_provider: Some("Anthropic"),
        status: UsageSnapshotStatus::Stale,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at: PARITY_NOW - 90_000,
        buckets: vec![ParityBucket {
            slot: Some(StatusSlot::Weekly),
            used_label: Some("70% used"),
            limit_label: Some("100%"),
            reset_at: Some(PARITY_NOW + 50_000),
            status: UsageSnapshotStatus::Stale,
            ..ParityBucket::metered("Weekly", 30)
        }],
        last_error: Some("showing last cached quota"),
        account_issues: Vec::new(),
        credential_expires_at: None,
        extra_groups: Vec::new(),
    }
}

/// Full mega-inventory: every scenario provider in one projection.
fn parity_mega_providers() -> Vec<ParityProvider> {
    vec![
        ParityProvider {
            provider_id: "antigravity",
            display_name: "Antigravity",
            accounts: vec![parity_antigravity_account()],
            provider_issues: Vec::new(),
        },
        ParityProvider {
            provider_id: "anthropic",
            display_name: "Anthropic",
            accounts: vec![
                parity_claude_work_account(),
                parity_claude_personal_account(),
            ],
            provider_issues: Vec::new(),
        },
        ParityProvider {
            provider_id: "cursor",
            display_name: "Cursor",
            accounts: vec![parity_cursor_account()],
            provider_issues: Vec::new(),
        },
        ParityProvider {
            provider_id: "openai",
            display_name: "OpenAI",
            accounts: vec![parity_exhausted_account()],
            provider_issues: Vec::new(),
        },
        ParityProvider {
            provider_id: "xai",
            display_name: "xAI",
            accounts: vec![parity_partial_account()],
            provider_issues: vec![parity_issue(
                "provider_slow",
                UsageIssueScopeV1::Provider,
                UsageIssueRecoverabilityV1::Retryable,
                "provider responding slowly",
                Some(PARITY_NOW + 630),
            )],
        },
        ParityProvider {
            provider_id: "minimax",
            display_name: "MiniMax",
            accounts: vec![parity_unsupported_account()],
            provider_issues: Vec::new(),
        },
        ParityProvider {
            provider_id: "zai",
            display_name: "Z.AI",
            accounts: vec![parity_auth_account()],
            provider_issues: Vec::new(),
        },
        ParityProvider {
            provider_id: "kimi",
            display_name: "Kimi",
            accounts: vec![parity_error_account()],
            provider_issues: Vec::new(),
        },
        ParityProvider {
            provider_id: "opencode",
            display_name: "OpenCode",
            accounts: vec![parity_zero_account()],
            provider_issues: Vec::new(),
        },
    ]
}

fn parity_unresolved_entries() -> Vec<UsageUnresolvedV1> {
    vec![
        UsageUnresolvedV1 {
            provider_id: "anthropic".to_owned(),
            capability_id: "anthropic:key".to_owned(),
            configuration_count: 1,
            state: UsageLifecycleV1::NeedsLogin,
            issues: vec![parity_issue(
                "auth_required",
                UsageIssueScopeV1::Account,
                UsageIssueRecoverabilityV1::ActionRequired,
                "authentication required",
                None,
            )],
        },
        UsageUnresolvedV1 {
            provider_id: "openai".to_owned(),
            capability_id: "openai:second".to_owned(),
            configuration_count: 1,
            state: UsageLifecycleV1::NeedsLogin,
            issues: Vec::new(),
        },
    ]
}

fn parity_projection_issue() -> UsageIssueV1 {
    parity_issue(
        "broker_degraded",
        UsageIssueScopeV1::Projection,
        UsageIssueRecoverabilityV1::Retryable,
        "one provider refresh failed",
        None,
    )
}

use jackin_console::tui::screens::usage::{
    UsageScreenState, UsageWindow, freshness_age_label, group_freshness_label,
};

fn parity_single_provider(provider: ParityProvider) -> (UsageProjectionV1, Vec<FocusedUsageView>) {
    let (projection, views) = parity_projection(&[provider], Vec::new(), Vec::new());
    (projection, views.into_iter().next().unwrap())
}

#[test]
fn parity_antigravity_two_family_windows_groups_and_credits() {
    let (projection, _) = parity_single_provider(ParityProvider {
        provider_id: "antigravity",
        display_name: "Antigravity",
        accounts: vec![parity_antigravity_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts.len(), 1);
    let account = &screen.accounts[0];
    assert_eq!(account.provider, "Antigravity");
    assert_eq!(account.account, "pilot@example.test");
    assert!(!account.unresolved);
    assert_eq!(account.status, "Available");
    assert_eq!(account.lifecycle, UsageLifecycleV1::Available);

    // Principal windows: family membership, order, percents, resets.
    let labels = account
        .windows
        .iter()
        .map(|window| window.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        [
            "Gemini · 5h",
            "Gemini · Weekly",
            "Other models · 5h",
            "Other models · Weekly"
        ]
    );
    let meters = account
        .windows
        .iter()
        .map(UsageWindow::meter_percent)
        .collect::<Vec<_>>();
    assert_eq!(meters, [Some(73), Some(41), Some(12), Some(88)]);
    assert_eq!(account.windows[0].value, "73% left");
    assert_eq!(account.windows[0].reset_at_epoch, Some(PARITY_NOW + 5_430));
    assert_eq!(account.windows[1].reset_at_epoch, Some(PARITY_NOW + 90_000));
    assert!(
        account
            .windows
            .iter()
            .all(|window| window.quota_state == UsageQuotaStateV1::Available)
    );

    // Metric groups: four real window groups, the real plan group, and the
    // hand-built balance group (no producer emits balances yet).
    assert_eq!(account.metric_groups.len(), 6);
    for (index, percent) in [73_u8, 41, 12, 88].iter().enumerate() {
        let group = &account.metric_groups[index];
        assert_eq!(group.kind, UsageMetricGroupKindV1::Window);
        assert_eq!(group.meter_percent(), Some(*percent));
        assert!(
            matches!(
                &group.value,
                UsageMetricValueV1::Window {
                    remaining_percent: Some(remaining),
                    ..
                } if remaining.get() == *percent
            ),
            "window group {index} must carry {percent}% remaining"
        );
    }
    assert_eq!(account.metric_groups[4].kind, UsageMetricGroupKindV1::Plan);
    let balance = &account.metric_groups[5];
    assert_eq!(balance.kind, UsageMetricGroupKindV1::Balance);
    assert_eq!(balance.label, "Credits");
    assert_eq!(balance.scope.pool.as_deref(), Some("credits-pool"));
    assert!(
        matches!(
            &balance.value,
            UsageMetricValueV1::Balance {
                amount,
                expires_at_epoch: Some(expires),
            } if amount == &usd(1_250) && *expires == PARITY_NOW + 2_592_000
        ),
        "balance group must carry $12.50 with expiry"
    );

    // Freshness: same age, renderer-owned words differ (see delta test).
    assert_eq!(freshness_age_label(PARITY_NOW, account), "updated 5m ago");
    assert_eq!(
        group_freshness_label(PARITY_NOW, &account.metric_groups[0]),
        "updated 5m ago"
    );
}

#[test]
fn parity_antigravity_capsule_tabs_and_detail() {
    let (_, views) = parity_single_provider(ParityProvider {
        provider_id: "antigravity",
        display_name: "Antigravity",
        accounts: vec![parity_antigravity_account()],
        provider_issues: Vec::new(),
    });

    // Capsule side: one tab, four bucket rows, same percents/resets.
    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    assert_eq!(view.tabs.len(), 1);
    assert_eq!(view.tabs[0].label, "Antigravity · pilot@example.test");
    assert!(view.tabs[0].active);
    assert_eq!(view.tabs[0].plan_label.as_deref(), Some("Antigravity Pro"));
    assert_eq!(
        view.tabs[0].source_label.as_deref(),
        Some("fresh · managed CLI")
    );
    // Most-constrained fresh bucket wins the tab status with model-bottleneck
    // naming (documented delta vs the console first-window summary).
    assert!(
        view.tabs[0]
            .status_label
            .starts_with("Other models · 5h 12% left"),
        "tab status names the bottleneck: {}",
        view.tabs[0].status_label
    );
    assert!(
        view.tabs[0].status_label.contains("Resets in 1h 30m"),
        "tab status carries the reset: {}",
        view.tabs[0].status_label
    );

    let detail = usage_detail_presentation(view);
    let row_labels = detail
        .rows
        .iter()
        .map(|row| row.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        row_labels,
        [
            "Plan",
            "Auth",
            "Gemini · 5h",
            "Gemini · Weekly",
            "Other models · 5h",
            "Other models · Weekly"
        ]
    );
    let bucket_rows = detail
        .rows
        .iter()
        .filter(|row| row.kind == UsageDetailRowKind::Bucket)
        .collect::<Vec<_>>();
    assert_eq!(bucket_rows.len(), 4);
    for (row, percent) in bucket_rows.iter().zip([73_u8, 41, 12, 88]) {
        assert_eq!(row.meter_percent, Some(percent));
        assert!(
            row.display_label.starts_with(&format!("{percent}% left")),
            "bucket display must lead with the percent: {}",
            row.display_label
        );
    }
    // Same reset epoch, renderer-owned formats (see delta test).
    assert!(
        bucket_rows[0].display_label.contains("Resets in 1h 30m"),
        "capsule reset label: {}",
        bucket_rows[0].display_label
    );

    let identity = usage_identity_presentation(
        provider_display_label(&view.account.provider_label),
        view,
        false,
    );
    assert_eq!(identity.provider_title, "Antigravity");
    assert_eq!(identity.account_label, "pilot@example.test");
    assert_eq!(identity.activity_label, "Updated 5m ago");
    assert_eq!(identity.activity_kind, UsageActivityKind::Idle);
}

#[test]
fn parity_duplicate_provider_accounts_stay_distinct() {
    let work = parity_claude_work_account();
    let personal = parity_claude_personal_account();
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "anthropic",
        display_name: "Anthropic",
        accounts: vec![work.clone(), personal.clone()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts.len(), 2);
    // Projection order is preserved; stable ids never collide.
    assert_eq!(screen.accounts[0].account, "work@example.test");
    assert_eq!(screen.accounts[1].account, "personal@example.test");
    assert_ne!(
        screen.accounts[0].stable_id(),
        screen.accounts[1].stable_id()
    );
    assert_ne!(
        screen.accounts[0].canonical_account_id,
        screen.accounts[1].canonical_account_id
    );
    // Identity evidence kinds survive: provider id vs stable handle.
    assert_eq!(
        screen.accounts[0].identity_kind,
        Some(UsageIdentityKindV1::ProviderAccountId)
    );
    assert_eq!(
        screen.accounts[1].identity_kind,
        Some(UsageIdentityKindV1::ProviderStableHandle)
    );

    // Capsule tabs: one per account, distinct ids, exact-id active marking.
    let enriched = parity_tabs(&views);
    let tabs = &enriched[0].tabs;
    assert_eq!(tabs.len(), 2);
    let ids = tabs
        .iter()
        .map(|tab| tab.id.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), 2, "tab ids must be distinct");
    assert_eq!(tabs[0].label, "Anthropic · personal@example.test");
    assert_eq!(tabs[1].label, "Anthropic · work@example.test");
    assert_eq!(
        enriched[0]
            .tabs
            .iter()
            .filter(|tab| tab.active)
            .map(|tab| tab.account_label.as_str())
            .collect::<Vec<_>>(),
        ["work@example.test"]
    );
    assert_eq!(
        enriched[1]
            .tabs
            .iter()
            .filter(|tab| tab.active)
            .map(|tab| tab.account_label.as_str())
            .collect::<Vec<_>>(),
        ["personal@example.test"]
    );

    // Duplicate snapshots for one account collapse to the newest fetch.
    let mut stale_work = parity_view(&work);
    stale_work.fetched_at_epoch = PARITY_NOW - 9_999;
    stale_work.account.plan_label = Some("Max OLD".to_owned());
    let collapsed = provider_tabs(&[&stale_work, &views[0], &views[1]]);
    assert_eq!(collapsed.len(), 2);
    let work_tab = collapsed
        .iter()
        .find(|tab| tab.account_label == "work@example.test")
        .unwrap();
    assert_eq!(work_tab.plan_label.as_deref(), Some("Max 20x"));

    // Legacy provider-label remap is shared and stable.
    assert_eq!(provider_display_label("Anthropic / Claude"), "Anthropic");
    assert_eq!(provider_display_label("Anthropic"), "Anthropic");
}

#[test]
fn parity_cursor_groups_money_and_units() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "cursor",
        display_name: "Cursor",
        accounts: vec![parity_cursor_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let account = &screen.accounts[0];
    let labels = account
        .windows
        .iter()
        .map(|window| window.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        ["Billing cycle", "Spend (actual)", "Credits", "Requests"]
    );
    let meters = account
        .windows
        .iter()
        .map(UsageWindow::meter_percent)
        .collect::<Vec<_>>();
    assert_eq!(meters, [Some(62), Some(55), None, Some(76)]);
    // API-mirrored Warn severity becomes a Warning quota state.
    assert_eq!(account.windows[0].quota_state, UsageQuotaStateV1::Warning);
    assert_eq!(
        account.windows[0].reset_at_epoch,
        Some(PARITY_NOW + 1_200_000)
    );
    // Used-only money still counts as quantity (Available), with no bar.
    assert_eq!(account.windows[2].quota_state, UsageQuotaStateV1::Available);
    assert_eq!(account.windows[2].value, "$8.30");

    // Groups: window, window, spend, window, spend, window, plan, rate-limit.
    let kinds = account
        .metric_groups
        .iter()
        .map(|group| group.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            UsageMetricGroupKindV1::Window,
            UsageMetricGroupKindV1::Window,
            UsageMetricGroupKindV1::SpendCap,
            UsageMetricGroupKindV1::Window,
            UsageMetricGroupKindV1::SpendCap,
            UsageMetricGroupKindV1::Window,
            UsageMetricGroupKindV1::Plan,
            UsageMetricGroupKindV1::RateLimit,
        ]
    );
    // Spend amounts stay in minor-unit scale end to end ($45.20, not $4520).
    match &account.metric_groups[2].value {
        UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } => {
            assert_eq!(
                cap.as_ref().map(ToString::to_string).as_deref(),
                Some("$100.00")
            );
            assert_eq!(
                spent.as_ref().map(ToString::to_string).as_deref(),
                Some("$45.20")
            );
            assert_eq!(
                remaining.as_ref().map(ToString::to_string).as_deref(),
                Some("$54.80")
            );
        }
        other => panic!("expected spend-cap value, got {other:?}"),
    }
    // Balance-shaped money (used-only) yields a cap-less spend group
    // (documented delta: renders as "uncapped" — see the render smoke test).
    match &account.metric_groups[4].value {
        UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } => {
            assert_eq!(cap, &None);
            assert_eq!(spent.as_ref(), Some(&usd(830)));
            assert_eq!(remaining, &None);
        }
        other => panic!("expected spend-cap value, got {other:?}"),
    }
    match &account.metric_groups[7].value {
        UsageMetricValueV1::RateLimit {
            limit,
            remaining,
            window_label,
        } => {
            assert_eq!(*limit, Some(100));
            assert_eq!(*remaining, Some(20));
            assert_eq!(window_label.as_deref(), Some("per minute"));
        }
        other => panic!("expected rate-limit value, got {other:?}"),
    }
    assert_eq!(
        account.metric_groups[7].reset_at_epoch,
        Some(PARITY_NOW + 90)
    );
    assert_eq!(freshness_age_label(PARITY_NOW, account), "updated 10m ago");

    // Capsule side: same amounts, same units.
    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    let spend = usage_bucket_presentation(&view.buckets[1]);
    assert!(
        spend.display_label.contains("$45.20"),
        "{}",
        spend.display_label
    );
    assert!(
        spend.display_label.contains("$100.00"),
        "{}",
        spend.display_label
    );
    let credits = usage_bucket_presentation(&view.buckets[2]);
    assert_eq!(credits.meter_percent, None);
    assert!(
        credits.display_label.contains("$8.30"),
        "{}",
        credits.display_label
    );
    let requests = usage_bucket_presentation(&view.buckets[3]);
    assert_eq!(requests.meter_percent, Some(76));
    assert!(
        requests.display_label.starts_with("76% left"),
        "{}",
        requests.display_label
    );
    let identity = usage_identity_presentation(
        provider_display_label(&view.account.provider_label),
        view,
        false,
    );
    assert_eq!(identity.activity_label, "Updated 10m ago");
}

#[test]
fn parity_exhausted_zero_is_not_unknown() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "openai",
        display_name: "OpenAI",
        accounts: vec![parity_exhausted_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let account = &screen.accounts[0];
    // Fresh account, exhausted window: a legit zero, never unknown.
    assert_eq!(account.status, "Available");
    assert_eq!(account.windows.len(), 1);
    assert_eq!(account.windows[0].meter_percent(), Some(0));
    assert_eq!(account.windows[0].value, "0% left");
    assert_eq!(account.windows[0].quota_state, UsageQuotaStateV1::Exhausted);
    assert_ne!(account.windows[0].quota_state, UsageQuotaStateV1::Unknown);
    // Missing plan label yields no plan group and no invented values.
    assert_eq!(account.metric_groups.len(), 1);
    assert_eq!(account.plan_label, None);

    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    let bucket = usage_bucket_presentation(&view.buckets[0]);
    assert_eq!(bucket.meter_percent, Some(0));
    assert!(
        bucket.display_label.starts_with("0% left"),
        "{}",
        bucket.display_label
    );
    assert!(
        view.tabs[0].status_label.starts_with("0% left"),
        "{}",
        view.tabs[0].status_label
    );
}

#[test]
fn parity_stale_partial_windows_issues_and_retry() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "xai",
        display_name: "xAI",
        accounts: vec![parity_partial_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let account = &screen.accounts[0];
    // Stale but Available: the status override names staleness honestly.
    assert_eq!(account.status, "stale");
    assert!(account.is_stale);
    assert_eq!(account.freshness_phase, UsageFreshnessPhaseV1::Stale);
    assert_eq!(
        freshness_age_label(PARITY_NOW, account),
        "stale · updated 25m ago"
    );
    // Retained last-good windows survive the partial failure.
    assert_eq!(account.windows.len(), 3);
    assert_eq!(account.windows[0].meter_percent(), Some(54));
    assert_eq!(account.windows[0].reset_at_epoch, Some(PARITY_NOW + 70_000));
    // Limit-only balance: usable quantity, but no percent means no bar.
    assert_eq!(account.windows[1].quota_state, UsageQuotaStateV1::Available);
    assert_eq!(account.windows[1].meter_percent(), None);
    // Quantity-less window: unknown, never a fabricated 0% bar.
    assert_eq!(account.windows[2].quota_state, UsageQuotaStateV1::Unknown);
    assert_eq!(account.windows[2].meter_percent(), None);
    // Freshness is per group: every group is stale with the same age.
    for group in &account.metric_groups {
        assert_eq!(
            group_freshness_label(PARITY_NOW, group),
            "stale · updated 25m ago"
        );
    }
    // The rate-limit issue keeps its code and broker retry.
    assert_eq!(account.issues.len(), 1);
    assert_eq!(account.issues[0].code, "rate_limited");
    assert_eq!(account.issues[0].retry_at_epoch, Some(PARITY_NOW + 330));
    assert_eq!(account.issue_count(), 1);

    // Capsule side: same windows, same staleness, degraded gracefully.
    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    assert_eq!(view.tabs[0].status_label, "stale");
    assert_eq!(
        view.tabs[0].source_label.as_deref(),
        Some("stale · provider")
    );
    let weekly = usage_bucket_presentation(&view.buckets[0]);
    assert_eq!(weekly.meter_percent, Some(54));
    assert!(
        weekly.display_label.contains("54% left"),
        "{}",
        weekly.display_label
    );
    assert!(
        weekly.display_label.contains("stale"),
        "{}",
        weekly.display_label
    );
    let credits = usage_bucket_presentation(&view.buckets[1]);
    assert_eq!(credits.meter_percent, None);
    assert!(
        credits.display_label.contains("$5.00"),
        "{}",
        credits.display_label
    );
    let unknown = usage_bucket_presentation(&view.buckets[2]);
    assert_eq!(unknown.meter_percent, None);
    assert!(
        !unknown.display_label.contains("0%"),
        "{}",
        unknown.display_label
    );
    let detail = usage_detail_presentation(view);
    let last = detail.rows.last().unwrap();
    assert_eq!(last.label, "Detail");
    assert_eq!(
        last.display_label,
        "rate limited by provider; showing last cached quota"
    );
    let identity = usage_identity_presentation(
        provider_display_label(&view.account.provider_label),
        view,
        false,
    );
    assert_eq!(identity.activity_label, "Update delayed · Updated 25m ago");
    assert_eq!(identity.activity_kind, UsageActivityKind::Exceptional);
}

#[test]
fn parity_unsupported_lifecycle_words() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "minimax",
        display_name: "MiniMax",
        accounts: vec![parity_unsupported_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let account = &screen.accounts[0];
    assert_eq!(account.lifecycle, UsageLifecycleV1::Unsupported);
    // Projection-owned status word (capitalized — see the wording delta).
    assert_eq!(account.status, "Unsupported");
    assert_eq!(account.freshness_phase, UsageFreshnessPhaseV1::Failed);
    assert_eq!(freshness_age_label(PARITY_NOW, account), "never updated");
    assert_eq!(account.windows.len(), 1);
    assert_eq!(
        account.windows[0].quota_state,
        UsageQuotaStateV1::Unsupported
    );
    assert_eq!(account.windows[0].meter_percent(), None);

    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    assert_eq!(view.tabs[0].status_label, "unsupported");
    assert_eq!(
        view.tabs[0].source_label.as_deref(),
        Some("unsupported · no source")
    );
    let bucket = usage_bucket_presentation(&view.buckets[0]);
    assert_eq!(bucket.meter_percent, None);
    assert_eq!(bucket.display_label, "unsupported");
    let detail = usage_detail_presentation(view);
    let row_labels = detail
        .rows
        .iter()
        .map(|row| row.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(row_labels, ["Quota", "Detail"]);
    let identity = usage_identity_presentation(
        provider_display_label(&view.account.provider_label),
        view,
        false,
    );
    assert_eq!(identity.activity_label, "Usage limits unsupported");
    assert_eq!(identity.activity_kind, UsageActivityKind::Exceptional);
}

#[test]
fn parity_auth_expired_login_state() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "zai",
        display_name: "Z.AI",
        accounts: vec![parity_auth_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let account = &screen.accounts[0];
    assert_eq!(account.lifecycle, UsageLifecycleV1::NeedsLogin);
    assert_eq!(account.status, "Needs login");
    assert!(account.windows.is_empty());
    assert!(account.metric_groups.is_empty());
    assert_eq!(
        account.credential_expires_at_epoch,
        Some(PARITY_NOW - 3_600)
    );
    assert_eq!(account.issues.len(), 1);
    assert_eq!(account.issues[0].code, "auth_required");
    // Source-capability subjects map to the stable-handle evidence kind.
    assert_eq!(
        account.identity_kind,
        Some(UsageIdentityKindV1::ProviderStableHandle)
    );

    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    assert_eq!(view.tabs[0].status_label, "needs login");
    assert_eq!(
        view.tabs[0].source_label.as_deref(),
        Some("needs login · no source")
    );
    // No buckets, no identity extras: only the error detail row remains.
    let detail = usage_detail_presentation(view);
    assert_eq!(detail.rows.len(), 1);
    assert_eq!(detail.rows[0].label, "Detail");
    assert_eq!(detail.rows[0].display_label, "sign in required");
    let identity = usage_identity_presentation(
        provider_display_label(&view.account.provider_label),
        view,
        false,
    );
    assert_eq!(identity.activity_label, "Sign in required");
    assert_eq!(identity.activity_kind, UsageActivityKind::Exceptional);
}

#[test]
fn parity_error_timeout_and_malformed() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "kimi",
        display_name: "Kimi",
        accounts: vec![parity_error_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let account = &screen.accounts[0];
    assert_eq!(account.lifecycle, UsageLifecycleV1::Error);
    assert_eq!(account.status, "Error");
    assert_eq!(freshness_age_label(PARITY_NOW, account), "never updated");
    assert_eq!(account.issue_count(), 2);
    assert_eq!(account.issues[0].code, "timeout");
    assert_eq!(account.issues[0].retry_at_epoch, Some(PARITY_NOW + 150));
    assert_eq!(account.issues[1].code, "malformed");
    assert_eq!(account.issues[1].retry_at_epoch, None);

    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    assert_eq!(view.tabs[0].status_label, "error");
    assert_eq!(
        view.tabs[0].source_label.as_deref(),
        Some("error · no source")
    );
    let detail = usage_detail_presentation(view);
    let last = detail.rows.last().unwrap();
    assert_eq!(last.display_label, "usage request timed out");
    let identity = usage_identity_presentation(
        provider_display_label(&view.account.provider_label),
        view,
        false,
    );
    assert_eq!(identity.activity_label, "Update failed · Error");
    assert_eq!(identity.activity_kind, UsageActivityKind::Exceptional);
}

#[test]
fn parity_legit_zero_spend_and_missing_fields() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "opencode",
        display_name: "OpenCode",
        accounts: vec![parity_zero_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let account = &screen.accounts[0];
    // The empty display label is preserved, never replaced.
    assert_eq!(account.account, "");
    assert_eq!(account.plan_label, None);
    assert_eq!(account.windows.len(), 1);
    assert_eq!(account.windows[0].meter_percent(), Some(100));
    assert_eq!(account.windows[0].value, "100% left");
    // Zero spend is tracked data: cap, spent, and remaining all survive.
    assert_eq!(account.metric_groups.len(), 2);
    match &account.metric_groups[1].value {
        UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } => {
            assert_eq!(spent.as_ref(), Some(&usd(0)));
            assert_eq!(cap.as_ref(), Some(&usd(10_000)));
            assert_eq!(remaining.as_ref(), Some(&usd(10_000)));
        }
        other => panic!("expected spend-cap value, got {other:?}"),
    }

    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    // Empty identity degrades honestly: bare provider tab, no invented name.
    assert_eq!(view.tabs[0].label, "OpenCode");
    assert_eq!(view.tabs[0].account_label, "account unavailable");
    assert!(view.tabs[0].active);
    assert_eq!(
        view.tabs[0].source_label.as_deref(),
        Some("fresh · local estimate")
    );
    let spend = usage_bucket_presentation(&view.buckets[0]);
    assert_eq!(spend.remaining_label.as_deref(), Some("0% used"));
    assert!(
        spend.display_label.contains("$0.00"),
        "{}",
        spend.display_label
    );
    assert!(
        spend.display_label.contains("$100.00"),
        "{}",
        spend.display_label
    );
    // No username/plan/auth/error: a single bucket row, nothing fabricated.
    let detail = usage_detail_presentation(view);
    assert_eq!(detail.rows.len(), 1);
    assert_eq!(detail.rows[0].kind, UsageDetailRowKind::Bucket);
    let identity = usage_identity_presentation(
        provider_display_label(&view.account.provider_label),
        view,
        false,
    );
    assert_eq!(identity.account_label, "No authenticated account");
    assert_eq!(identity.activity_label, "Updated now");
    assert_eq!(identity.activity_kind, UsageActivityKind::Idle);
}

#[test]
fn parity_s4_empty_inventory() {
    let (projection, views_by_provider) = parity_projection(&[], Vec::new(), Vec::new());
    assert!(views_by_provider.is_empty());
    let screen = UsageScreenState::from_projection(&projection);
    assert!(screen.accounts.is_empty());
    assert_eq!(screen.notice, None);
    assert_eq!(screen.generated_at_epoch, Some(PARITY_NOW));
    assert!(screen.projection_issues.is_empty());

    // Empty scope stays empty: no tabs, no rows, no invented providers.
    assert!(provider_tabs(&[]).is_empty());
    assert!(parity_tabs(&[]).is_empty());

    let text = parity_render_text(screen, 100, 24);
    assert!(
        text.contains("No providers configured"),
        "empty inventory must explain itself:\n{text}"
    );
}

#[test]
fn parity_s5_stale_and_partial_failure() {
    let (projection, views_by_provider) = parity_projection(
        &[ParityProvider {
            provider_id: "anthropic",
            display_name: "Anthropic",
            accounts: vec![parity_claude_work_account(), parity_old_stale_account()],
            provider_issues: Vec::new(),
        }],
        Vec::new(),
        vec![parity_projection_issue()],
    );
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts.len(), 2);
    // Fresh and stale siblings keep independent freshness: neither clears
    // the other, and the projection issue does not rewrite either status.
    let fresh = &screen.accounts[0];
    assert_eq!(fresh.status, "Available");
    assert!(!fresh.is_stale);
    assert_eq!(freshness_age_label(PARITY_NOW, fresh), "updated 2m ago");
    assert!(
        fresh
            .metric_groups
            .iter()
            .all(|group| !group.is_stale && group.phase == UsageFreshnessPhaseV1::Current)
    );
    let stale = &screen.accounts[1];
    assert_eq!(stale.status, "stale");
    assert!(stale.is_stale);
    assert_eq!(
        freshness_age_label(PARITY_NOW, stale),
        "stale · updated 1d ago"
    );
    assert!(
        stale
            .metric_groups
            .iter()
            .all(|group| group.is_stale && group.phase == UsageFreshnessPhaseV1::Stale)
    );
    assert_eq!(screen.projection_issues.len(), 1);
    assert_eq!(screen.projection_issues[0].code, "broker_degraded");

    let views = views_by_provider.into_iter().next().unwrap();
    let enriched = parity_tabs(&views);
    assert_eq!(enriched.len(), 2);
    let fresh_tabs = &enriched[0].tabs;
    assert_eq!(fresh_tabs.len(), 2);
    let stale_tab = fresh_tabs
        .iter()
        .find(|tab| tab.account_label == "old@example.test")
        .unwrap();
    assert_eq!(stale_tab.status_label, "stale");
    assert_eq!(stale_tab.source_label.as_deref(), Some("stale · provider"));
    let work_tab = fresh_tabs
        .iter()
        .find(|tab| tab.account_label == "work@example.test")
        .unwrap();
    assert!(
        work_tab.status_label.contains("% left"),
        "{}",
        work_tab.status_label
    );
    assert_eq!(work_tab.source_label.as_deref(), Some("fresh · provider"));
    let stale_identity = usage_identity_presentation(
        provider_display_label(&enriched[1].account.provider_label),
        &enriched[1],
        false,
    );
    assert_eq!(
        stale_identity.activity_label,
        "Update delayed · Updated 25h ago"
    );
}

#[test]
fn parity_unresolved_stays_console_only() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "openai",
        display_name: "OpenAI",
        accounts: vec![parity_exhausted_account()],
        provider_issues: Vec::new(),
    });
    let mut projection = projection;
    projection.unresolved = parity_unresolved_entries();
    projection.validate().unwrap();
    let screen = UsageScreenState::from_projection(&projection);
    // One resolved row plus two unresolved rows grouped by provider.
    assert_eq!(screen.accounts.len(), 3);
    assert_eq!(screen.accounts[0].provider, "OpenAI");
    assert_eq!(screen.accounts[0].account, "zero@example.test");
    assert!(!screen.accounts[0].unresolved);
    assert_eq!(screen.accounts[1].provider, "OpenAI");
    assert_eq!(screen.accounts[1].account, "Unresolved (openai:second)");
    assert_eq!(screen.accounts[1].status, "needs login");
    assert!(screen.accounts[1].unresolved);
    assert_eq!(screen.accounts[1].stable_id(), "openai:openai:second");
    assert_eq!(screen.accounts[2].provider, "Anthropic");
    assert_eq!(screen.accounts[2].account, "Unresolved (anthropic:key)");
    assert_eq!(
        screen.accounts[2].status,
        "needs login · authentication required"
    );
    assert!(screen.accounts[2].unresolved);
    assert_eq!(screen.accounts[2].identity_kind, None);
    assert_eq!(
        screen.notice.as_deref(),
        Some("2 configured capability(s) unresolved")
    );

    // Capsule rows require resolved launch membership: a capability alone
    // never creates a tab (N7).
    let enriched = parity_tabs(&views);
    assert_eq!(enriched[0].tabs.len(), 1);
    assert_eq!(enriched[0].tabs[0].account_label, "zero@example.test");
}

fn parity_render_text(screen_state: UsageScreenState, width: u16, height: u16) -> String {
    use jackin_console::tui::state::ManagerState;
    use ratatui::{Terminal, backend::TestBackend};

    let config = jackin_config::AppConfig::default();
    let mut manager = ManagerState::from_config(&config, std::path::Path::new("/test"));
    manager.usage.screen = Some(screen_state);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            jackin_console::tui::screens::usage::render(frame, frame.area(), &manager);
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn documented_delta_overview_summary_selection() {
    // Console list summaries read the FIRST window (73%); capsule tab
    // statuses read the MOST-CONSTRAINED fresh bucket ("Other models · 5h
    // 12% left"). D30 wants the first ranked real limit; the capsule keeps
    // Bug-5 most-constrained. Disposition: RECORDED — needs a cross-surface
    // owner decision; the capsule selection lives in jackin-usage view.rs,
    // outside renderer scope, so neither owned renderer can converge alone.
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "antigravity",
        display_name: "Antigravity",
        accounts: vec![parity_antigravity_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        screen.accounts[0].windows[0].meter_percent(),
        Some(73),
        "console summary input is the first window"
    );
    let enriched = parity_tabs(&views);
    assert!(
        enriched[0].tabs[0]
            .status_label
            .starts_with("Other models · 5h 12% left"),
        "capsule summary is the most-constrained bucket: {}",
        enriched[0].tabs[0].status_label
    );
}

#[test]
fn documented_delta_spend_meter_direction() {
    // Console spend bars fill by REMAINING; capsule spend bars fill by USED
    // ($45.20/$100: console 55 vs capsule 45; $0/$100: console 100 vs
    // capsule 0). Disposition: RECORDED — console windows carry no spend
    // marker (category `Other`), so the console renderer cannot distinguish
    // spend; converging needs a protocol marker or a shared-presentation
    // change, both outside renderer scope.
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "cursor",
        display_name: "Cursor",
        accounts: vec![parity_cursor_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        screen.accounts[0].windows[1].meter_percent(),
        Some(55),
        "console spend meter fills by remaining"
    );
    let spend = usage_bucket_presentation(&views[0].buckets[1]);
    assert_eq!(
        spend.meter_percent,
        Some(45),
        "capsule spend meter fills by used"
    );
    assert_eq!(spend.remaining_label.as_deref(), Some("45% used"));

    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "opencode",
        display_name: "OpenCode",
        accounts: vec![parity_zero_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].windows[0].meter_percent(), Some(100));
    assert_eq!(
        usage_bucket_presentation(&views[0].buckets[0]).meter_percent,
        Some(0)
    );
}

#[test]
fn documented_delta_severity_color_inputs() {
    // The console meter color is quota-state-driven (fixed in this change);
    // the capsule accent is API-severity-driven. The projection maps
    // `Danger`→`Exhausted` and `Warn`→`Warning`, so both renderers agree
    // whenever the mapping holds. Pinned here for the three fixture
    // severities; the console mapping itself is pinned by the colocated
    // console regression test.
    let (projection, _) = parity_single_provider(ParityProvider {
        provider_id: "anthropic",
        display_name: "Anthropic",
        accounts: vec![parity_claude_personal_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        screen.accounts[0].windows[1].quota_state,
        UsageQuotaStateV1::Exhausted,
        "Danger severity must surface as Exhausted"
    );
    let (projection, _) = parity_single_provider(ParityProvider {
        provider_id: "cursor",
        display_name: "Cursor",
        accounts: vec![parity_cursor_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        screen.accounts[0].windows[0].quota_state,
        UsageQuotaStateV1::Warning,
        "Warn severity must surface as Warning"
    );
    let (projection, _) = parity_single_provider(ParityProvider {
        provider_id: "antigravity",
        display_name: "Antigravity",
        accounts: vec![parity_antigravity_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        screen.accounts[0].windows[2].quota_state,
        UsageQuotaStateV1::Available,
        "Normal severity stays Available even at 12% (both renderers green)"
    );
}

#[test]
fn documented_delta_reset_and_freshness_wording() {
    // Same epochs, renderer-owned formats. Window reset strings are verbatim
    // projection copies (EQUAL on both surfaces); console group schedule
    // lines use countdown buckets while capsule buckets use countdown plus a
    // local timestamp. Freshness ages agree below 24h modulo case, then the
    // buckets diverge (console day bucket vs capsule 25h form).
    // Disposition: RECORDED (accepted renderer-owned formats; every epoch is
    // asserted equal here).
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "antigravity",
        display_name: "Antigravity",
        accounts: vec![parity_antigravity_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let console_window = &screen.accounts[0].windows[0];
    let capsule_bucket = &views[0].buckets[0];
    assert_eq!(console_window.reset_at_epoch, capsule_bucket.resets_at);
    assert_eq!(console_window.reset_at_epoch, Some(PARITY_NOW + 5_430));
    assert_eq!(
        console_window.reset,
        capsule_bucket.reset_label.clone().unwrap()
    );
    assert!(
        capsule_bucket
            .reset_label
            .clone()
            .unwrap()
            .starts_with("Resets in 1h 30m"),
        "capsule countdown form: {:?}",
        capsule_bucket.reset_label
    );
    assert_eq!(
        freshness_age_label(PARITY_NOW, &screen.accounts[0]),
        "updated 5m ago"
    );
    assert_eq!(views[0].updated_label, "Updated 5m ago");

    // Day-old staleness: console day bucket vs capsule hour count.
    let (projection, views_by_provider) = parity_projection(
        &[ParityProvider {
            provider_id: "anthropic",
            display_name: "Anthropic",
            accounts: vec![parity_old_stale_account()],
            provider_issues: Vec::new(),
        }],
        Vec::new(),
        Vec::new(),
    );
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        freshness_age_label(PARITY_NOW, &screen.accounts[0]),
        "stale · updated 1d ago"
    );
    let views = views_by_provider.into_iter().next().unwrap();
    assert_eq!(views[0].updated_label, "Updated 25h ago");
}

#[test]
fn documented_delta_status_wording() {
    // Healthy: console "Available" (projection status label) vs capsule
    // "fresh" (freshness vocabulary). Failure words match modulo case:
    // console "Needs login"/"Unsupported"/"Error" vs capsule lowercase.
    // Stale matches exactly ("stale"). Disposition: RECORDED —
    // projection-owned strings plus the renderer's stale override; the
    // renderer must not rewrite canonical copy, so unifying needs a
    // projection/console vocabulary decision.
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "openai",
        display_name: "OpenAI",
        accounts: vec![parity_exhausted_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].status, "Available");
    let enriched = parity_tabs(&views);
    assert_eq!(
        enriched[0].tabs[0].source_label.as_deref(),
        Some("fresh · provider")
    );

    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "minimax",
        display_name: "MiniMax",
        accounts: vec![parity_unsupported_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].status, "Unsupported");
    let enriched = parity_tabs(&views);
    assert_eq!(enriched[0].tabs[0].status_label, "unsupported");

    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "zai",
        display_name: "Z.AI",
        accounts: vec![parity_auth_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].status, "Needs login");
    let enriched = parity_tabs(&views);
    assert_eq!(enriched[0].tabs[0].status_label, "needs login");

    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "kimi",
        display_name: "Kimi",
        accounts: vec![parity_error_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].status, "Error");
    let enriched = parity_tabs(&views);
    assert_eq!(enriched[0].tabs[0].status_label, "error");

    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "xai",
        display_name: "xAI",
        accounts: vec![parity_partial_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].status, "stale");
    let enriched = parity_tabs(&views);
    assert_eq!(enriched[0].tabs[0].status_label, "stale");
}

#[test]
fn documented_delta_credential_expiry_capsule_gap() {
    // Console surfaces the credential-expiry epoch ("Credential expired 1h
    // ago" — pinned in the render smoke test); the capsule identity
    // presentation has no expiry field because `FocusedUsageView` carries
    // none. Disposition: RECORDED — structural view-protocol gap.
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "zai",
        display_name: "Z.AI",
        accounts: vec![parity_auth_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        screen.accounts[0].credential_expires_at_epoch,
        Some(PARITY_NOW - 3_600)
    );
    let enriched = parity_tabs(&views);
    let identity = usage_identity_presentation(
        provider_display_label(&enriched[0].account.provider_label),
        &enriched[0],
        false,
    );
    assert_eq!(identity.activity_label, "Sign in required");
    assert!(
        !format!("{identity:?}").contains("expir"),
        "capsule identity must not invent expiry text: {identity:?}"
    );
}

#[test]
fn documented_delta_issue_retry_capsule_gap() {
    // Console issues keep stable codes plus the broker retry ("retry in 5m" /
    // "retry in 2m" — pinned in the render smoke test); capsule surfaces
    // only the bare `last_error` string. Disposition: RECORDED — the view
    // carries no retry epoch.
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "xai",
        display_name: "xAI",
        accounts: vec![parity_partial_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].issues[0].code, "rate_limited");
    assert_eq!(
        screen.accounts[0].issues[0].retry_at_epoch,
        Some(PARITY_NOW + 330)
    );
    let detail = usage_detail_presentation(&views[0]);
    let last = detail.rows.last().unwrap();
    assert_eq!(
        last.display_label,
        "rate limited by provider; showing last cached quota"
    );
    assert!(
        !last.display_label.contains("retry"),
        "{}",
        last.display_label
    );

    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "kimi",
        display_name: "Kimi",
        accounts: vec![parity_error_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].issues[0].code, "timeout");
    assert_eq!(
        screen.accounts[0].issues[0].retry_at_epoch,
        Some(PARITY_NOW + 150)
    );
    let detail = usage_detail_presentation(&views[0]);
    assert_eq!(
        detail.rows.last().unwrap().display_label,
        "usage request timed out"
    );
}

#[test]
fn documented_delta_balance_value_and_uncapped_spend() {
    // Limit-only balances (the Grok prepaid seam): the console window value
    // is blank (the projection falls back to `used_label` only) while the
    // capsule shows "$5.00" via its balance seam. Used-only money yields a
    // cap-less console spend group ("uncapped · spent $8.30" — pinned in the
    // render smoke test). Disposition: RECORDED — projection-owned
    // (`value_label` fallback; slot-blind spend-group emission); the console
    // cannot recover either from its inputs.
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "xai",
        display_name: "xAI",
        accounts: vec![parity_partial_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].windows[1].value, "");
    let credits = usage_bucket_presentation(&views[0].buckets[1]);
    assert!(
        credits.display_label.contains("$5.00"),
        "{}",
        credits.display_label
    );

    let (projection, _) = parity_single_provider(ParityProvider {
        provider_id: "cursor",
        display_name: "Cursor",
        accounts: vec![parity_cursor_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert!(
        matches!(
            &screen.accounts[0].metric_groups[4].value,
            UsageMetricValueV1::SpendCap {
                cap: None,
                spent: Some(_),
                ..
            }
        ),
        "balance-shaped money yields a cap-less spend group"
    );
}

#[test]
fn documented_delta_model_scope_capsule_gap() {
    // Console groups carry model/pool scope ("scope: pool credits-pool" —
    // pinned in the detail render); capsule buckets and detail rows have no
    // scope surface. (Related: the console detail pane has no username row
    // while capsule does — the projection never carries `username`.)
    // Disposition: RECORDED — `QuotaBucketView` lacks scope axes and the
    // projection drops `username`; both need protocol changes.
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "antigravity",
        display_name: "Antigravity",
        accounts: vec![parity_antigravity_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        screen.accounts[0].metric_groups[5].scope.pool.as_deref(),
        Some("credits-pool")
    );
    let detail = usage_detail_presentation(&views[0]);
    assert!(
        detail
            .rows
            .iter()
            .all(|row| !row.display_label.contains("credits-pool")),
        "capsule rows have no scope surface"
    );

    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "anthropic",
        display_name: "Anthropic",
        accounts: vec![parity_claude_work_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        screen.accounts[0].metric_groups[3].scope.model.as_deref(),
        Some("claude-opus-4-6")
    );
    let detail = usage_detail_presentation(&views[0]);
    assert!(
        detail
            .rows
            .iter()
            .all(|row| !row.display_label.contains("claude-opus-4-6")),
        "capsule rows have no scope surface"
    );
    assert!(
        detail
            .rows
            .iter()
            .any(|row| row.label == "Username" && row.display_label == "work-user"),
        "capsule keeps the username row the console projection drops"
    );
}

#[test]
fn documented_delta_overage_magnitude() {
    // $150 against a $100 cap: the console preserves raw 150 ("150% used" +
    // the raw note — pinned in the render smoke test) while the capsule
    // spend presentation caps at "100% used". Disposition: RECORDED — the
    // capsule spend presentation has no raw-percent channel.
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "anthropic",
        display_name: "Anthropic",
        accounts: vec![parity_claude_personal_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let window = &screen.accounts[0].windows[2];
    assert_eq!(window.used_percent, Some(100));
    assert_eq!(window.used_raw_percent, Some(150));
    assert_eq!(window.remaining_percent, None);
    assert_eq!(window.value, "150% used");
    assert_eq!(window.quota_state, UsageQuotaStateV1::Exhausted);
    let spend = usage_bucket_presentation(&views[0].buckets[2]);
    assert_eq!(spend.remaining_label.as_deref(), Some("100% used"));
    assert_eq!(spend.meter_percent, Some(100));
}

#[test]
fn documented_delta_meter_percent_inputs() {
    // Meter PERCENT inputs agree modulo the documented spend direction:
    // every non-spend capsule meter equals the console window meter, and
    // every spend meter is its mirror (100 − p). Rendered glyphs match
    // statically: the console `meter_line` and the capsule full-width meter
    // both draw `█`/`░` (the capsule's intermediate `·` empty cell never
    // reaches the screen).
    let providers = parity_mega_providers();
    let defs = providers
        .iter()
        .flat_map(|provider| provider.accounts.iter())
        .collect::<Vec<_>>();
    let (projection, views_by_provider) = parity_projection(&providers, Vec::new(), Vec::new());
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts.len(), defs.len());
    let views = views_by_provider.iter().flatten().collect::<Vec<_>>();
    for ((def, view), console) in defs.iter().zip(views.iter()).zip(screen.accounts.iter()) {
        assert_eq!(
            console.windows.len(),
            view.buckets.len(),
            "window/bucket count for {}",
            def.account_label
        );
        for ((bucket_def, bucket), window) in def
            .buckets
            .iter()
            .zip(view.buckets.iter())
            .zip(console.windows.iter())
        {
            let capsule_meter = usage_bucket_presentation(bucket).meter_percent;
            let console_meter = window.meter_percent();
            if bucket_def.slot == Some(StatusSlot::Spend) {
                assert_eq!(
                    console_meter,
                    capsule_meter.map(|meter| 100 - meter),
                    "spend mirror for {}",
                    bucket_def.label
                );
            } else {
                assert_eq!(
                    console_meter, capsule_meter,
                    "meter agreement for {}",
                    bucket_def.label
                );
            }
        }
    }
}

#[test]
fn documented_delta_error_buckets_carry_no_percent() {
    // The quota-driven console color and the severity-driven capsule accent
    // can only disagree on error-status buckets that still carry a percent;
    // no harness error/login/unsupported bucket does (adapters omit percents
    // there). Disposition: RECORDED residual with fixture evidence.
    let providers = parity_mega_providers();
    let (_, views_by_provider) = parity_projection(&providers, Vec::new(), Vec::new());
    for view in views_by_provider.iter().flatten() {
        for bucket in &view.buckets {
            if matches!(
                bucket.status,
                UsageSnapshotStatus::Error
                    | UsageSnapshotStatus::NeedsLogin
                    | UsageSnapshotStatus::NeedsSecret
                    | UsageSnapshotStatus::Unsupported
            ) {
                assert_eq!(
                    bucket.remaining_percent, None,
                    "error-status bucket {} must not carry a percent",
                    bucket.label
                );
            }
        }
    }
    // The auth and error fixtures carry no buckets at all.
    let auth_view = parity_view(&parity_auth_account());
    assert!(auth_view.buckets.is_empty());
    let error_view = parity_view(&parity_error_account());
    assert!(error_view.buckets.is_empty());
}

/// Wall clock for render tests only. The console renderer hardcodes wall
/// time for relative labels, so render fixtures are shifted to wall time
/// while every structural assertion keeps the fixed `PARITY_NOW`.
fn parity_wall_now() -> i64 {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
}

/// Shift every absolute timestamp in one fixture account by `delta`,
/// preserving all ages and countdowns.
fn parity_shift_account(account: &ParityAccount, delta: i64) -> ParityAccount {
    let mut shifted = account.clone();
    shifted.fetched_at += delta;
    for bucket in &mut shifted.buckets {
        if let Some(reset) = bucket.reset_at.as_mut() {
            *reset += delta;
        }
    }
    for issue in &mut shifted.account_issues {
        if let Some(retry) = issue.retry_at_epoch.as_mut() {
            *retry += delta;
        }
    }
    if let Some(expires) = shifted.credential_expires_at.as_mut() {
        *expires += delta;
    }
    for group in &mut shifted.extra_groups {
        group.fetched_at += delta;
        for epoch in [
            &mut group.observed_at,
            &mut group.last_success_at,
            &mut group.reset_at,
            &mut group.renews_at,
        ] {
            if let Some(epoch) = epoch.as_mut() {
                *epoch += delta;
            }
        }
        if let UsageMetricValueV1::Balance {
            expires_at_epoch: Some(expires),
            ..
        } = &mut group.value
        {
            *expires += delta;
        }
    }
    shifted
}

fn parity_shift_providers(providers: &[ParityProvider], delta: i64) -> Vec<ParityProvider> {
    providers
        .iter()
        .map(|provider| ParityProvider {
            provider_id: provider.provider_id,
            display_name: provider.display_name,
            accounts: provider
                .accounts
                .iter()
                .map(|account| parity_shift_account(account, delta))
                .collect(),
            provider_issues: provider
                .provider_issues
                .iter()
                .map(|issue| {
                    let mut shifted = issue.clone();
                    if let Some(retry) = shifted.retry_at_epoch.as_mut() {
                        *retry += delta;
                    }
                    shifted
                })
                .collect(),
        })
        .collect()
}

fn parity_mega_screen() -> UsageScreenState {
    // Wall-relative: the renderer reads wall time for relative labels.
    // Fixture offsets sit mid-bucket (≥30s from every edge), so the render
    // that follows within milliseconds cannot straddle a bucket boundary.
    let now = parity_wall_now();
    let providers = parity_shift_providers(&parity_mega_providers(), now - PARITY_NOW);
    let (projection, _) = parity_projection_at(
        now,
        &providers,
        parity_unresolved_entries(),
        vec![parity_projection_issue()],
    );
    UsageScreenState::from_projection(&projection)
}

#[test]
fn parity_console_render_smoke_overview() {
    // Renderer-private strings pinned end to end: every scenario provider,
    // both unresolved rows, the notice, and the projection issue.
    let text = parity_render_text(parity_mega_screen(), 150, 240);
    for expected in [
        // Antigravity two-family fixture.
        "Antigravity · pilot@example.test",
        "Gemini · 5h",
        "73% left",
        "Other models · 5h",
        "12% left",
        "Gemini · 5h: 73% left · provider-defined period",
        "Gemini · Weekly: 41% left · weekly",
        "Other models · 5h: 12% left",
        "resets in 1h",
        "resets in 1d",
        "resets in 13d",
        "Plan: Antigravity Pro",
        "Credits: $12.50",
        "expires in 30d",
        // Duplicate provider accounts plus overage.
        "Anthropic · work@example.test",
        "Anthropic · personal@example.test",
        "150% used",
        "raw used 150%",
        // Cursor groups and exact money units.
        "Cursor · cursor-user",
        "Billing cycle",
        "Spend (actual) spend: cap $100.00 · spent $45.20 · remaining $54.80",
        "Credits spend: uncapped · spent $8.30",
        "API rate limit: limit 100 · remaining 20 · per minute",
        "resets in 1m",
        // Legit-zero exhaustion.
        "OpenAI · zero@example.test",
        "0% left",
        "quota: exhausted",
        // Stale partial failure with retry + provider issue + unknown.
        "xAI · partial@example.test",
        "stale · updated 25m ago",
        "rate limited by provider (rate_limited) · retry in 5m",
        "provider: provider responding slowly (provider_slow) · retry in 10m",
        "quota: unknown",
        // Unsupported / auth-expired / hard-error states.
        "MiniMax · mm-user",
        "quota: unsupported",
        "Z.AI · zai-user",
        "Needs login",
        "Credential expired 1h ago",
        "sign in required (auth_required)",
        "Kimi · kimi-user",
        "usage request timed out (timeout) · retry in 2m",
        "usage response malformed (malformed)",
        // Legit-zero spend with missing identity fields.
        "OpenCode · ",
        "100% left",
        "Tokens spend: cap $100.00 · spent $0.00 · remaining $100.00",
        // Unresolved rows, notice, and projection issue.
        "Unresolved (anthropic:key)",
        "needs login · authentication required",
        "Unresolved (openai:second)",
        "2 configured capability(s) unresolved",
        "one provider refresh failed (broker_degraded)",
    ] {
        assert!(
            text.contains(expected),
            "overview render must contain {expected:?}:\n{text}"
        );
    }
}

#[test]
fn parity_console_render_smoke_detail_scopes() {
    // Group scope lines only render in the account detail pane.
    let mut screen = parity_mega_screen();
    screen.selected = 1;
    screen.detail = true;
    let text = parity_render_text(screen, 120, 70);
    for expected in [
        "Provider  Antigravity",
        "Account   pilot@example.test",
        "Status    Available",
        "Plan      Antigravity Pro",
        "Identity  provider handle",
        "Freshness updated 5m ago",
        "73% left · Resets in 1h 30m",
        "pace: On pace",
        "Credits (balance · available · updated 5m ago)",
        "scope: pool credits-pool",
        "expires in 30d",
        "fetched 5m ago",
    ] {
        assert!(
            text.contains(expected),
            "antigravity detail must contain {expected:?}:\n{text}"
        );
    }

    let mut screen = parity_mega_screen();
    screen.selected = 2;
    screen.detail = true;
    let text = parity_render_text(screen, 120, 70);
    for expected in [
        "Tokens (token totals · n/a · updated 2m ago)",
        "scope: model claude-opus-4-6",
        "input 1500000 · output 320000 · cached 900000 · this week",
        "Max 20x",
    ] {
        assert!(
            text.contains(expected),
            "work detail must contain {expected:?}:\n{text}"
        );
    }
    // The projection never carries `username`: the console detail pane has
    // no username row while the capsule keeps one (protocol gap, noted in
    // the parity report).
    assert!(
        !text.contains("work-user"),
        "console detail must not invent a username row:\n{text}"
    );
}

#[test]
fn parity_s5_render_smoke() {
    let now = parity_wall_now();
    let providers = parity_shift_providers(
        &[ParityProvider {
            provider_id: "anthropic",
            display_name: "Anthropic",
            accounts: vec![parity_claude_work_account(), parity_old_stale_account()],
            provider_issues: Vec::new(),
        }],
        now - PARITY_NOW,
    );
    let (projection, _) =
        parity_projection_at(now, &providers, Vec::new(), vec![parity_projection_issue()]);
    let screen = UsageScreenState::from_projection(&projection);
    let text = parity_render_text(screen, 120, 50);
    for expected in [
        "Anthropic · work@example.test",
        "updated 2m ago",
        "Anthropic · old@example.test",
        "stale · updated 1d ago",
        "one provider refresh failed (broker_degraded)",
    ] {
        assert!(
            text.contains(expected),
            "S5 render must contain {expected:?}:\n{text}"
        );
    }
}
