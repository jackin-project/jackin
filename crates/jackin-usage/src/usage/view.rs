// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! View-building and rendering helpers shared by all providers.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.

#[cfg_attr(
    not(test),
    expect(clippy::wildcard_imports, reason = "target-dependent")
)]
use super::*;

impl UsageCache {
    /// Adopt one host-broker generation without executing provider work locally.
    pub fn adopt_broker_generation(
        &mut self,
        target: &UsageRefreshTarget,
        state: &jackin_protocol::usage_broker::UsageGenerationView,
    ) {
        if target.capability != state.capability
            || !capability_matches_surface(
                &target.agent,
                target.provider.as_deref(),
                &state.capability,
            )
        {
            return;
        }
        let mut view = state.snapshot.clone().unwrap_or_else(|| {
            if state.phase.is_active() {
                FocusedUsageView::refreshing(target.provider.as_deref(), now_epoch())
            } else {
                FocusedUsageView::unavailable(
                    state
                        .error
                        .as_ref()
                        .map_or("usage coordinator unavailable", |error| {
                            error.message.as_str()
                        }),
                    now_epoch(),
                )
            }
        });
        if let Some(error) = &state.error {
            view.last_error = Some(error.message.clone());
            view.status = if view.buckets.is_empty() {
                UsageSnapshotStatus::Error
            } else {
                UsageSnapshotStatus::Stale
            };
        }
        if view.focused_agent.is_none() {
            view.focused_agent = Some(target.agent.clone());
        }
        if view.focused_provider.is_none() {
            view.focused_provider = target.provider.clone();
        }
        if state.error.is_some() {
            refresh_failed_view_presentation(&mut view);
        }
        self.snapshots.insert(
            usage_cache_key_for_broker_account(
                &target.agent,
                target.provider.as_deref(),
                &state.capability,
            ),
            CachedUsage { view },
        );
    }

    /// Preserve last-good quota while surfacing a typed relay/broker failure.
    pub fn adopt_broker_error(
        &mut self,
        target: &UsageRefreshTarget,
        error: &jackin_protocol::usage_broker::UsageCoordinationError,
    ) {
        if !capability_matches_surface(
            &target.agent,
            target.provider.as_deref(),
            &target.capability,
        ) {
            return;
        }
        let cache_key = target.cache_key();
        let cached = self.snapshots.entry(cache_key).or_insert_with(|| {
            let mut view = FocusedUsageView::unavailable(&error.message, now_epoch());
            view.focused_agent = Some(target.agent.clone());
            view.focused_provider = target.provider.clone();
            CachedUsage { view }
        });
        cached.view.last_error = Some(error.message.clone());
        cached.view.status = if cached.view.buckets.is_empty() {
            UsageSnapshotStatus::Error
        } else {
            UsageSnapshotStatus::Stale
        };
        refresh_failed_view_presentation(&mut cached.view);
    }
}

/// Keep row-level status honest when a broker failure preserves last-good
/// buckets. A stale/error view must not render fresh bucket rows or an
/// "updated now" label.
fn refresh_failed_view_presentation(view: &mut FocusedUsageView) {
    if view.status != UsageSnapshotStatus::Fresh {
        for bucket in &mut view.buckets {
            bucket.status = view.status;
        }
    }
    view.updated_label = match view.status {
        UsageSnapshotStatus::Fresh => "Updated now",
        UsageSnapshotStatus::Stale => "Stale",
        UsageSnapshotStatus::NeedsLogin => "Needs login",
        UsageSnapshotStatus::NeedsSecret => "Needs secret",
        UsageSnapshotStatus::Unsupported => "Unsupported",
        UsageSnapshotStatus::Unavailable => "Unavailable",
        UsageSnapshotStatus::Error => "Error",
    }
    .to_owned();
    view.status_bar_label = status_bar_label(
        resolve_surface(
            view.focused_agent.as_deref().unwrap_or_default(),
            view.focused_provider.as_deref(),
        ),
        &view.account.account_label,
        view.status,
        &view.buckets,
    );
}

/// Stamp the surface-derived agent, provider label, and tab strip onto a base
/// placeholder view, so a `unavailable`/`refreshing` view still shows the proper
/// header (e.g. `Anthropic / Claude`) and tabs while it loads.
pub(crate) fn decorate_surface_view(
    view: &mut FocusedUsageView,
    agent: &str,
    focused_provider: Option<&str>,
    surface: UsageSurface,
) {
    view.focused_agent = Some(agent.to_owned());
    view.focused_provider = focused_provider
        .map(str::to_owned)
        .or_else(|| Some(surface.label().to_owned()));
    view.account.provider_label = surface.account_label().to_owned();
    // Tabs are per-account, built from admitted snapshots; a placeholder names
    // no account yet, so it carries none (empty scope stays empty).
    view.tabs = provider_tabs(&[]);
}

pub(crate) fn cached_unavailable_view(
    agent: &str,
    focused_provider: Option<&str>,
    now: i64,
) -> FocusedUsageView {
    let surface = resolve_surface(agent, focused_provider);
    let mut view =
        FocusedUsageView::unavailable("usage unavailable: no cached provider snapshot", now);
    decorate_surface_view(&mut view, agent, focused_provider, surface);
    view
}

pub(crate) fn cached_refreshing_view(
    agent: &str,
    focused_provider: Option<&str>,
    now: i64,
) -> FocusedUsageView {
    let surface = resolve_surface(agent, focused_provider);
    let mut view = FocusedUsageView::refreshing(focused_provider, now);
    decorate_surface_view(&mut view, agent, focused_provider, surface);
    view
}

pub(crate) fn mark_active_tab(view: &mut FocusedUsageView) {
    // Navigation keys on the stable canonical account id, never on the
    // display label: same-provider accounts share a label but never an id.
    let focused = usage_account_tab_id(&view.account.provider_label, &view.account.account_label);
    for tab in &mut view.tabs {
        tab.active = tab.id == focused;
    }
}

pub(crate) fn account_snapshot_views_from_cache(
    snapshots: &HashMap<String, CachedUsage>,
) -> Vec<AccountUsageSnapshotView> {
    let mut accounts = snapshots
        .values()
        .flat_map(|cached| {
            let view = &cached.view;
            view.buckets.iter().map(|bucket| {
                let (used_amount, used_unit, limit_amount, limit_unit) =
                    quota_amounts_for_account_snapshot(bucket);
                let status =
                    if snapshot_status_rank(bucket.status) > snapshot_status_rank(view.status) {
                        bucket.status
                    } else {
                        view.status
                    };
                AccountUsageSnapshotView {
                    provider: view.account.provider_label.clone(),
                    account_label: view.account.account_label.clone(),
                    source: usage_source_storage_label(view.source).to_owned(),
                    confidence: usage_confidence_storage_label(view.confidence).to_owned(),
                    window_kind: bucket.label.clone(),
                    used_amount,
                    used_unit,
                    limit_amount,
                    limit_unit,
                    resets_at: bucket.resets_at,
                    fetched_at: view.fetched_at_epoch,
                    expires_at: None,
                    status: usage_status_storage_label(status).to_owned(),
                    last_error: view.last_error.clone(),
                }
            })
        })
        .collect::<Vec<_>>();
    accounts.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then(left.window_kind.cmp(&right.window_kind))
    });
    accounts
}

pub(crate) fn quota_amounts_for_account_snapshot(
    bucket: &QuotaBucketView,
) -> (Option<i64>, Option<String>, Option<i64>, Option<String>) {
    if bucket.used_money.is_some() || bucket.limit_money.is_some() {
        return (
            bucket.used_money.as_ref().map(|money| money.amount_minor),
            bucket
                .used_money
                .as_ref()
                .map(|money| money.currency.clone()),
            bucket.limit_money.as_ref().map(|money| money.amount_minor),
            bucket
                .limit_money
                .as_ref()
                .map(|money| money.currency.clone()),
        );
    }
    if bucket.status_slot == Some(StatusSlot::Spend) {
        return (None, None, None, None);
    }
    let Some(remaining) = bucket.remaining_percent else {
        return (None, None, None, None);
    };
    (
        Some(i64::from(100_u8.saturating_sub(remaining.min(100)))),
        Some("percent".to_owned()),
        Some(100),
        Some("percent".to_owned()),
    )
}

const fn snapshot_status_rank(status: UsageSnapshotStatus) -> u8 {
    match status {
        UsageSnapshotStatus::Fresh => 0,
        UsageSnapshotStatus::Stale => 1,
        UsageSnapshotStatus::NeedsLogin => 2,
        UsageSnapshotStatus::NeedsSecret => 3,
        UsageSnapshotStatus::Unsupported => 4,
        UsageSnapshotStatus::Unavailable => 5,
        UsageSnapshotStatus::Error => 6,
    }
}

pub(crate) struct UsageViewInput<'a> {
    pub(crate) agent: &'a str,
    pub(crate) provider: Option<&'a str>,
    pub(crate) surface: UsageSurface,
    pub(crate) account_label: String,
    pub(crate) username: Option<String>,
    pub(crate) plan_label: Option<String>,
    pub(crate) credential_origin: Option<String>,
    pub(crate) buckets: Vec<QuotaBucketView>,
    pub(crate) status: UsageSnapshotStatus,
    pub(crate) source: UsageSource,
    pub(crate) confidence: UsageConfidence,
    pub(crate) now: i64,
    pub(crate) last_error: Option<String>,
}

pub(crate) fn usage_view(input: UsageViewInput<'_>) -> FocusedUsageView {
    let headline = status_bar_label(
        input.surface,
        &input.account_label,
        input.status,
        &input.buckets,
    );
    let mut view = FocusedUsageView {
        focused_agent: Some(input.agent.to_owned()),
        focused_provider: input
            .provider
            .map(str::to_owned)
            .or_else(|| Some(input.surface.label().to_owned())),
        account: FocusedAccountHeader {
            provider_label: input.surface.account_label().to_owned(),
            account_label: input.account_label,
            username: input.username,
            plan_label: input.plan_label,
            credential_origin: input.credential_origin,
        },
        buckets: input.buckets,
        status: input.status,
        source: input.source,
        confidence: input.confidence,
        fetched_at_epoch: input.now,
        updated_label: match input.status {
            UsageSnapshotStatus::Fresh => "Updated now",
            UsageSnapshotStatus::Stale => "Stale",
            UsageSnapshotStatus::NeedsLogin => "Needs login",
            UsageSnapshotStatus::NeedsSecret => "Needs secret",
            UsageSnapshotStatus::Unsupported => "Unsupported",
            UsageSnapshotStatus::Unavailable => "Unavailable",
            UsageSnapshotStatus::Error => "Error",
        }
        .to_owned(),
        status_bar_label: headline,
        tabs: Vec::new(),
        last_error: input.last_error,
    };
    // A freshly built view tabs its own account; the cache enriches the strip
    // to every admitted account before display.
    view.tabs = provider_tabs(&[&view]);
    view
}

/// Monetary spend for the status-bar headline, read from the `Spend`-slot
/// bucket and rendered `<used> of <limit>` with the currency shown once
/// (e.g. `SGD 78 of 260`). `None` unless a fresh/stale bucket carries
/// structured [`Money`], so the headline shows nothing rather than a stale or
/// zeroed figure.
pub(crate) fn spend_headline_label(buckets: &[QuotaBucketView]) -> Option<String> {
    let spend = buckets.iter().find(|bucket| {
        bucket.status_slot == Some(StatusSlot::Spend) && status_bar_fresh_or_stale(bucket)
    })?;
    let used = spend.used_money.as_ref()?;
    // Drop zero spend from the compact headline (Bug 8): `$0 spent` / `$0 of N`
    // carries no signal in the status bar. The dialog still shows `$0.00 spent`.
    if used.amount_minor == 0 {
        return None;
    }
    Some(match spend.limit_money.as_ref() {
        Some(limit) => format!("{} of {}", used.format_compact(), limit.major_amount()),
        None => format!("{} spent", used.format_compact()),
    })
}

pub(crate) fn status_bar_label(
    surface: UsageSurface,
    _account_label: &str,
    status: UsageSnapshotStatus,
    buckets: &[QuotaBucketView],
) -> String {
    if let Some(headline) = status_bar_headline_for_surface(surface, buckets) {
        return headline;
    }
    match status {
        UsageSnapshotStatus::Fresh => "usage cached".to_owned(),
        UsageSnapshotStatus::Stale => "stale".to_owned(),
        UsageSnapshotStatus::NeedsLogin => "login".to_owned(),
        UsageSnapshotStatus::NeedsSecret => "secret".to_owned(),
        UsageSnapshotStatus::Unsupported => "unsupported".to_owned(),
        UsageSnapshotStatus::Unavailable => "usage unavailable".to_owned(),
        UsageSnapshotStatus::Error => "error".to_owned(),
    }
}

pub(crate) fn status_bar_headline_for_surface(
    surface: UsageSurface,
    buckets: &[QuotaBucketView],
) -> Option<String> {
    if surface == UsageSurface::Amp {
        amp_status_bar_headline(buckets)
    } else {
        // Session/Weekly percentages, then the monetary spend, all in one
        // ` · `-joined headline (e.g. `Session 89% · Weekly 73% · SGD 78 of 260`).
        let mut labels = status_bar_quota_labels(buckets);
        labels.extend(spend_headline_label(buckets));
        (!labels.is_empty()).then(|| labels.join(" · "))
    }
}

pub(crate) fn amp_status_bar_headline(buckets: &[QuotaBucketView]) -> Option<String> {
    // Daily is the only Amp glance headline; credit/workspace bounds stay
    // detail-only and never leak into the status bar or infer availability
    // from a bucket title.
    buckets
        .iter()
        .find(|bucket| {
            status_bar_fresh_or_stale(bucket) && bucket.status_slot == Some(StatusSlot::Daily)
        })
        .and_then(|bucket| {
            bucket
                .remaining_percent
                .map(|remaining| format!("Free {remaining}%"))
        })
}

pub(crate) fn status_bar_quota_labels(buckets: &[QuotaBucketView]) -> Vec<String> {
    // Read the semantic slot the provider tagged at construction, not the
    // free-text label — a window rename can't silently break the headline.
    [
        (StatusSlot::Session, "Session"),
        (StatusSlot::Weekly, "Weekly"),
    ]
    .into_iter()
    .filter_map(|(slot, label)| {
        buckets
            .iter()
            .find(|bucket| bucket.status_slot == Some(slot) && status_bar_fresh_or_stale(bucket))
            .and_then(|bucket| {
                // Drop a zero window from the compact headline (Bug 8, operator
                // decision: omit every zero-value segment from the status bar;
                // the dialog still shows `0% left`).
                bucket
                    .remaining_percent
                    .filter(|&remaining| remaining != 0)
                    .map(|remaining| format!("{label} {remaining}%"))
            })
    })
    .collect()
}

pub(crate) fn status_bar_fresh_or_stale(bucket: &QuotaBucketView) -> bool {
    matches!(
        bucket.status,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
    )
}

pub(crate) fn compact_account_identity(account_label: &str) -> &str {
    let trimmed = account_label.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("needs ")
        || trimmed.ends_with(" unavailable")
        || trimmed.contains(" not available")
    {
        "account unavailable"
    } else {
        trimmed
    }
}

/// Rank of one bucket in the settled Overview-summary order (D30:
/// long-range weekly/daily, model-specific, session, then other). The slot
/// mapping mirrors the projection's `window_category` exactly
/// (`Spend`/`None` read as `Other`), so the capsule and the console select
/// the same window; no producer emits the `Model` category yet, so
/// model-specific windows rank as `Other` on both surfaces until one does.
fn summary_slot_rank(slot: Option<StatusSlot>) -> u8 {
    match slot {
        Some(StatusSlot::Daily | StatusSlot::Weekly) => 0,
        // No `StatusSlot` marks a model-specific window; unslotted buckets
        // rank as `Other`, exactly like the projection maps them.
        Some(StatusSlot::Session) => 2,
        Some(StatusSlot::Spend) | None => 3,
    }
}

pub(crate) fn summary_bucket(buckets: &[QuotaBucketView]) -> Option<&QuotaBucketView> {
    // First available Rust-ranked limit (D30): lowest category rank wins,
    // ties break to provider order, and only fresh buckets carrying a
    // remaining percent qualify. Spend ranks last as `Other` (Bug 5: a
    // reset-less spend bucket must not win the headline over a real limit),
    // but still wins over nothing, so a spend-only account shows its quota.
    buckets
        .iter()
        .enumerate()
        .filter(|(_, bucket)| bucket.status == UsageSnapshotStatus::Fresh)
        .filter(|(_, bucket)| bucket.remaining_percent.is_some())
        .min_by_key(|(index, bucket)| (summary_slot_rank(bucket.status_slot), *index))
        .map(|(_, bucket)| bucket)
}

pub(crate) fn preserve_cached_quota_on_failed_refresh(
    view: &mut FocusedUsageView,
    cached: &FocusedUsageView,
) {
    if !matches!(
        view.status,
        UsageSnapshotStatus::Stale | UsageSnapshotStatus::NeedsLogin | UsageSnapshotStatus::Error
    ) || cached.status != UsageSnapshotStatus::Fresh
        || cached.buckets.is_empty()
    {
        return;
    }

    view.status = UsageSnapshotStatus::Stale;
    view.source = UsageSource::Cache;
    view.confidence = cached.confidence;
    view.updated_label = "Stale".to_owned();
    view.buckets = cached
        .buckets
        .iter()
        .cloned()
        .map(|mut bucket| {
            bucket.status = UsageSnapshotStatus::Stale;
            bucket
        })
        .collect();
    if view.account.plan_label.is_none() {
        view.account.plan_label = cached.account.plan_label.clone();
    }
    if compact_account_identity(&view.account.account_label) == "account unavailable" {
        view.account.account_label = cached.account.account_label.clone();
    }
    if let Some(error) = &mut view.last_error {
        error.push_str("; showing last cached quota");
    } else {
        view.last_error = Some("showing last cached quota".to_owned());
    }
    view.status_bar_label = status_bar_label(
        resolve_surface(
            view.focused_agent.as_deref().unwrap_or_default(),
            view.focused_provider.as_deref(),
        ),
        &view.account.account_label,
        view.status,
        &view.buckets,
    );
}

/// Stable canonical account id keying usage tabs: the same
/// [`account_key_hash`] the durable snapshot store uses as its stable
/// multi-account id, so tabs, overview rows, and stored snapshots correlate.
/// Same-provider accounts share a display label but never an id.
pub(crate) fn usage_account_tab_id(provider_label: &str, account_label: &str) -> String {
    account_key_hash(provider_label, account_label)
}

/// One tab per distinct admitted account, keyed by
/// [`usage_account_tab_id`]. Duplicate snapshots for one account collapse to
/// the newest fetch; the strip sorts by display label, then account, then id,
/// so any provider (Cursor, `OpenRouter`, Copilot, Antigravity, Gemini,
/// `OpenCode`, omp, Hermes, …) tabs without a hardcoded surface list. Empty
/// input stays empty.
pub(crate) fn provider_tabs(views: &[&FocusedUsageView]) -> Vec<UsageProviderTab> {
    let mut keyed: Vec<(String, &FocusedUsageView)> = views
        .iter()
        .map(|view| {
            (
                usage_account_tab_id(&view.account.provider_label, &view.account.account_label),
                *view,
            )
        })
        .collect();
    // Newest fetch first per account, so `dedup_by` (which keeps the first of
    // each run) keeps the latest snapshot; full ties are interchangeable.
    keyed.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then(right.1.fetched_at_epoch.cmp(&left.1.fetched_at_epoch))
    });
    keyed.dedup_by(|left, right| left.0 == right.0);
    let mut tabs: Vec<UsageProviderTab> = keyed
        .into_iter()
        .map(|(id, view)| account_tab(id, view))
        .collect();
    tabs.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then(left.account_label.cmp(&right.account_label))
            .then(left.id.cmp(&right.id))
    });
    tabs
}

/// Tab enrichment is pure reconstruction: one tab per admitted account
/// snapshot, no hardcoded surface list, no fuzzy-label latest-wins. Active
/// marking is [`mark_active_tab`]'s exact-id job, applied after.
pub(crate) fn enrich_provider_tabs(
    view: &mut FocusedUsageView,
    snapshots: &HashMap<String, CachedUsage>,
) {
    let views: Vec<&FocusedUsageView> = snapshots.values().map(|cached| &cached.view).collect();
    view.tabs = provider_tabs(&views);
}

fn account_tab(id: String, view: &FocusedUsageView) -> UsageProviderTab {
    UsageProviderTab {
        id,
        label: account_tab_label(view),
        status_label: usage_tab_status_label(view),
        account_label: compact_account_identity(&view.account.account_label).to_owned(),
        plan_label: view.account.plan_label.clone(),
        source_label: Some(usage_tab_source_label(view)),
        active: false,
    }
}

/// Display label for an account tab: the snapshot's provider label (falling
/// back to the focused provider when the snapshot carries only the generic
/// `Usage` placeholder), suffixed with the compact account identity so
/// same-provider accounts render individually visible tabs. Matching still
/// keys on the id, never this label.
fn account_tab_label(view: &FocusedUsageView) -> String {
    account_tab_label_for_parts(
        &view.account.provider_label,
        &view.account.account_label,
        view.focused_provider.as_deref(),
    )
}

/// Shared tab-label rule (live cache and snapshot store): provider head with
/// a ` · {account}` suffix so same-provider accounts stay individually
/// visible.
pub(crate) fn account_tab_label_for_parts(
    provider_label: &str,
    account_label: &str,
    focused_provider: Option<&str>,
) -> String {
    let provider = provider_label.trim();
    let provider = if !provider.is_empty()
        && !provider.eq_ignore_ascii_case(UsageSurface::Unsupported.label())
    {
        provider.to_owned()
    } else {
        focused_provider
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .unwrap_or(UsageSurface::Unsupported.label())
            .to_owned()
    };
    let account = compact_account_identity(account_label);
    if account == "account unavailable" {
        provider
    } else {
        format!("{provider} · {account}")
    }
}

impl UsageCache {
    /// Focused snapshot for an exact canonical account id (tab selection).
    /// `None` when no admitted snapshot carries the id; the caller falls back
    /// to label resolution only for empty ids (old payloads).
    pub fn focused_snapshot_for_account_id(&self, account_id: &str) -> Option<FocusedUsageView> {
        let mut view = self
            .snapshots
            .values()
            .filter(|cached| {
                usage_account_tab_id(
                    &cached.view.account.provider_label,
                    &cached.view.account.account_label,
                ) == account_id
            })
            .max_by_key(|cached| cached.view.fetched_at_epoch)
            .map(|cached| cached.view.clone())?;
        refresh_cached_updated_label(&mut view, now_epoch());
        enrich_provider_tabs(&mut view, &self.snapshots);
        mark_active_tab(&mut view);
        Some(view)
    }

    /// Broker-namespace account id behind an exact tab id, recovered from the
    /// owning cache entry's broker key. `None` for unknown ids and for
    /// non-broker entries (legacy keys carry no capability).
    pub fn broker_account_id_for_tab_id(&self, tab_id: &str) -> Option<String> {
        self.snapshots
            .iter()
            .filter(|(_, cached)| {
                usage_account_tab_id(
                    &cached.view.account.provider_label,
                    &cached.view.account.account_label,
                ) == tab_id
            })
            .filter_map(|(key, cached)| {
                broker_account_id_from_cache_key(key).map(|id| (cached.view.fetched_at_epoch, id))
            })
            .max_by_key(|(fetched_at_epoch, _)| *fetched_at_epoch)
            .map(|(_, id)| id)
    }
}

/// Parse the broker account id out of a broker cache key
/// (`{base}:account-id-v1:{surface_id}:{account_id}`, built by
/// `usage_cache_key_for_broker_account`). Surface ids are closed colon-free
/// tokens, so the first colon after the marker splits the pair.
fn broker_account_id_from_cache_key(key: &str) -> Option<String> {
    let (_, rest) = key.split_once(":account-id-v1:")?;
    let (surface_id, account_id) = rest.split_once(':')?;
    (!surface_id.is_empty() && !account_id.is_empty()).then(|| account_id.to_owned())
}

/// Freshness + source tag for the Overview row, e.g. "fresh · provider" or
/// "stale · local estimate", matching the CodexBar-style status column.
pub(crate) fn usage_tab_source_label(view: &FocusedUsageView) -> String {
    let freshness = match view.status {
        UsageSnapshotStatus::Fresh => "fresh",
        UsageSnapshotStatus::Stale => "stale",
        UsageSnapshotStatus::NeedsLogin => "needs login",
        UsageSnapshotStatus::NeedsSecret => "needs secret",
        UsageSnapshotStatus::Unsupported => "unsupported",
        UsageSnapshotStatus::Unavailable => "unavailable",
        UsageSnapshotStatus::Error => "error",
    };
    let source = match view.source {
        UsageSource::ProviderApi => "provider",
        UsageSource::Cli => "managed CLI",
        UsageSource::LocalLogs => "local estimate",
        UsageSource::Cache => "cache",
        UsageSource::None => "no source",
    };
    format!("{freshness} · {source}")
}

pub(crate) fn usage_tab_status_label(view: &FocusedUsageView) -> String {
    if view.status == UsageSnapshotStatus::Fresh
        && let Some(bucket) = summary_bucket(&view.buckets)
        && let Some(remaining) = bucket.remaining_percent
    {
        // The summary window is the first available Rust-ranked limit (D30),
        // shared with the console list summary. An unslotted window (a
        // model-scoped Fable/Sonnet limit, or any other provider bucket)
        // winning the headline is named, so the Overview/status row tells the
        // operator *which* limit the % traces to, not just the % left.
        // Headline windows (Session/Weekly) stay bare: their slot already
        // implies them and the status bar carries those separately.
        let mut label = String::new();
        if bucket.status_slot.is_none() && !bucket.label.is_empty() {
            label.push_str(&bucket.label);
            label.push(' ');
        }
        label.push_str(&format!("{remaining}% left"));
        if let Some(reset) = &bucket.reset_label {
            label.push_str(" · ");
            label.push_str(reset);
        }
        return label;
    }
    match view.status {
        UsageSnapshotStatus::Fresh => "fresh".to_owned(),
        UsageSnapshotStatus::Stale => "stale".to_owned(),
        UsageSnapshotStatus::NeedsLogin => "needs login".to_owned(),
        UsageSnapshotStatus::NeedsSecret => "needs secret".to_owned(),
        UsageSnapshotStatus::Unsupported => "unsupported".to_owned(),
        UsageSnapshotStatus::Unavailable => "unavailable".to_owned(),
        UsageSnapshotStatus::Error => "error".to_owned(),
    }
}

pub(crate) fn bucket(
    label: &str,
    used_label: Option<String>,
    limit_label: Option<String>,
    remaining_percent: Option<u8>,
    reset_label: Option<String>,
    pace_label: Option<&str>,
    status: UsageSnapshotStatus,
) -> QuotaBucketView {
    QuotaBucketView {
        label: label.to_owned(),
        used_label,
        limit_label,
        remaining_percent,
        reset_label,
        resets_at: None,
        status_slot: None,
        pace_label: pace_label.map(str::to_owned),
        status,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
    }
}

/// Stamp a quota bucket's status-bar slot at construction. Returns the bucket so
/// it can be tagged and pushed in one expression (`buckets.push(with_status_slot(
/// build(...), Some(StatusSlot::Session)))`) — the slot rides with the view it
/// belongs to, so no later `last_mut`/positional step can float the tag onto the
/// wrong bucket.
pub(crate) fn with_status_slot(
    mut view: QuotaBucketView,
    slot: Option<StatusSlot>,
) -> QuotaBucketView {
    view.status_slot = slot;
    view
}

/// Build a window bucket carrying both the formatted reset label and the raw
/// reset epoch (RC2), so the CLI report can emit `resets_at`. `reset_at` is the
/// authoritative timestamp; `reset_label` is derived from it.
#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn timed_bucket(
    label: &str,
    used_label: Option<String>,
    limit_label: Option<String>,
    remaining_percent: Option<u8>,
    reset_at: Option<i64>,
    now: i64,
    pace_label: Option<&str>,
    status: UsageSnapshotStatus,
) -> QuotaBucketView {
    let mut view = bucket(
        label,
        used_label,
        limit_label,
        remaining_percent,
        reset_at.map(|epoch| reset_label(epoch, now)),
        pace_label,
        status,
    );
    view.resets_at = reset_at;
    view
}
