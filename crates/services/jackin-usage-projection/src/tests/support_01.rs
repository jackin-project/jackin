// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn sorted(locale: &str, labels: &[&str]) -> Vec<String> {
    let locale = locale.parse::<Locale>().expect("test locale");
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Secondary);
    let collator = Collator::try_new(locale.into(), options).expect("test collator");
    let mut labels = labels.iter().map(ToString::to_string).collect::<Vec<_>>();
    labels.sort_by(|left, right| collator.compare(left, right));
    labels
}

pub(super) fn bucket(label: &str) -> QuotaBucketView {
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

pub(super) fn view_with_buckets(
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

pub(super) fn catalog_entry(
    view: FocusedUsageView,
    plan_label: Option<&str>,
) -> AccountCatalogEntry {
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

pub(super) fn status_bucket(label: &str, status: UsageSnapshotStatus) -> QuotaBucketView {
    let mut bucket = bucket(label);
    bucket.remaining_percent = Some(57);
    bucket.status = status;
    bucket
}

pub(super) const PARITY_NOW: i64 = 1_800_000_000;

pub(super) const PARITY_GENERATION: u64 = 7;

#[derive(Debug, Clone)]
pub(super) struct ParityBucket {
    pub(super) label: &'static str,
    pub(super) slot: Option<StatusSlot>,
    pub(super) used_label: Option<&'static str>,
    pub(super) limit_label: Option<&'static str>,
    pub(super) remaining: Option<u8>,
    pub(super) reset_at: Option<i64>,
    pub(super) pace: Option<&'static str>,
    pub(super) status: UsageSnapshotStatus,
    pub(super) severity: UsageSeverity,
    pub(super) used_money: Option<Money>,
    pub(super) limit_money: Option<Money>,
}

impl ParityBucket {
    /// Minimal fresh bucket with a remaining percent and no reset/money.
    pub(super) fn metered(label: &'static str, remaining: u8) -> Self {
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

#[derive(Debug, Clone)]
pub(super) struct ParityAccount {
    pub(super) surface: HostSurfaceId,
    pub(super) provider_id: &'static str,
    pub(super) provider_label: &'static str,
    pub(super) subject: CanonicalAccountSubject,
    pub(super) account_key: &'static str,
    pub(super) account_label: &'static str,
    pub(super) username: Option<&'static str>,
    pub(super) plan_label: Option<&'static str>,
    pub(super) credential_origin: Option<&'static str>,
    /// Capsule provider label override for surfaces without a `UsageSurface`
    /// variant (Cursor, Antigravity); mirrors the adapter pattern of building
    /// with `Unsupported` and then stamping the label.
    pub(super) capsule_provider_label: Option<&'static str>,
    pub(super) agent: &'static str,
    pub(super) focused_provider: Option<&'static str>,
    pub(super) status: UsageSnapshotStatus,
    pub(super) source: UsageSource,
    pub(super) confidence: UsageConfidence,
    pub(super) fetched_at: i64,
    pub(super) buckets: Vec<ParityBucket>,
    pub(super) last_error: Option<&'static str>,
    /// Broker-owned overlays (populated post-projection, as the broker does).
    pub(super) account_issues: Vec<UsageIssueV1>,
    pub(super) credential_expires_at: Option<i64>,
    pub(super) extra_groups: Vec<ParityExtraGroup>,
}

#[derive(Debug, Clone)]
pub(super) struct ParityExtraGroup {
    pub(super) kind: UsageMetricGroupKindV1,
    pub(super) label: &'static str,
    pub(super) scope: UsageMetricScopeV1,
    pub(super) value: UsageMetricValueV1,
    pub(super) quota_state: UsageQuotaStateV1,
    pub(super) phase: UsageFreshnessPhaseV1,
    pub(super) is_stale: bool,
    pub(super) observed_at: Option<i64>,
    pub(super) fetched_at: i64,
    pub(super) last_success_at: Option<i64>,
    pub(super) reset_at: Option<i64>,
    pub(super) renews_at: Option<i64>,
}

pub(super) fn parity_issue(
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

pub(super) fn parity_bucket_view_at(def: &ParityBucket, now: i64) -> QuotaBucketView {
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

pub(super) fn parity_surface(provider_id: &str) -> UsageSurface {
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

pub(super) fn parity_view(account: &ParityAccount) -> FocusedUsageView {
    parity_view_at(account, PARITY_NOW)
}

pub(super) fn parity_view_at(account: &ParityAccount, now: i64) -> FocusedUsageView {
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

pub(super) fn parity_entry(account: &ParityAccount, view: FocusedUsageView) -> AccountCatalogEntry {
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

pub(super) fn parity_extra_group(
    canonical_id: &str,
    rank: u32,
    def: &ParityExtraGroup,
) -> UsageMetricGroupV1 {
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

pub(super) fn parity_project_account(
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

pub(super) struct ParityProvider {
    pub(super) provider_id: &'static str,
    pub(super) display_name: &'static str,
    pub(super) accounts: Vec<ParityAccount>,
    pub(super) provider_issues: Vec<UsageIssueV1>,
}

pub(super) fn parity_projection(
    providers: &[ParityProvider],
    unresolved: Vec<UsageUnresolvedV1>,
    projection_issues: Vec<UsageIssueV1>,
) -> (UsageProjectionV1, Vec<Vec<FocusedUsageView>>) {
    parity_projection_at(PARITY_NOW, providers, unresolved, projection_issues)
}

pub(super) fn parity_projection_at(
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

pub(super) fn parity_tabs(views: &[FocusedUsageView]) -> Vec<FocusedUsageView> {
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

pub(super) fn usd(cents: i64) -> Money {
    Money::new(cents, "USD", 2)
}
