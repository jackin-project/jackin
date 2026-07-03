//! View-building and rendering helpers shared by all providers.
//!
//! Carved out of `usage.rs` during codebase-health-enforcement Workstream W5
//! (file-size ratchet). Items in this module are `pub(crate)` so the
//! coordinator (`usage.rs`) can re-export them.

#[allow(clippy::wildcard_imports)]
use super::*;

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
    view.tabs = provider_tabs(surface);
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
    let provider = view.focused_provider.as_deref().unwrap_or_default();
    for tab in &mut view.tabs {
        tab.active = provider_matches_usage_label(&tab.label, provider)
            || provider_matches_usage_label(&tab.label, &view.account.provider_label);
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
                    status: usage_status_storage_label(bucket.status).to_owned(),
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
    FocusedUsageView {
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
            UsageSnapshotStatus::Fresh => "Updated just now",
            UsageSnapshotStatus::Stale => "Stale",
            UsageSnapshotStatus::NeedsLogin => "Needs login",
            UsageSnapshotStatus::NeedsSecret => "Needs secret",
            UsageSnapshotStatus::Unsupported => "Unsupported",
            UsageSnapshotStatus::Unavailable => "Unavailable",
            UsageSnapshotStatus::Error => "Error",
        }
        .to_owned(),
        status_bar_label: headline,
        tabs: provider_tabs(input.surface),
        last_error: input.last_error,
    }
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
    let free = buckets
        .iter()
        .find(|bucket| status_bar_fresh_or_stale(bucket) && bucket.label == "Amp Free")
        .and_then(|bucket| {
            bucket
                .remaining_percent
                .map(|remaining| format!("Free {remaining}%"))
        });
    let credits = buckets
        .iter()
        .find(|bucket| {
            status_bar_fresh_or_stale(bucket)
                && matches!(bucket.label.as_str(), "Individual credits" | "Credits")
        })
        .and_then(amp_credit_status_label);
    match (free, credits) {
        (Some(free), Some(credits)) => Some(format!("{free} · {credits}")),
        (Some(free), None) => Some(free),
        (None, Some(credits)) => Some(credits),
        (None, None) => None,
    }
}

pub(crate) fn amp_credit_status_label(bucket: &QuotaBucketView) -> Option<String> {
    bucket
        .limit_label
        .as_deref()
        .or_else(|| {
            bucket
                .pace_label
                .as_deref()
                .and_then(|label| label.strip_prefix("Individual credits: "))
        })
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_owned)
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

/// True when `word` appears in `text` as a whole alphanumeric token, so a short
/// provider token (`amp`) is not matched inside an unrelated word (`example`).
pub(crate) fn contains_word(text: &str, word: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|token| token == word)
}

/// Best-effort canonical surface for any provider-ish text — a tab label
/// (`OpenAI / Codex`) or an account provider label (`codex`), case-insensitive
/// and synonym-aware. `None` for text that names no known provider.
pub(crate) fn surface_from_text(text: &str) -> Option<UsageSurface> {
    let text = text.to_ascii_lowercase();
    UsageSurface::ALL.iter().copied().find(|&surface| {
        surface.synonyms().iter().any(|syn| {
            // Amp matches only on a word boundary so labels like `example` or
            // `ramp` don't false-link; every other token keeps the historical
            // case-insensitive substring policy.
            if matches!(surface, UsageSurface::Amp) {
                contains_word(&text, syn)
            } else {
                text.contains(syn)
            }
        })
    })
}

pub(crate) fn provider_matches_usage_label(provider: &str, account_provider: &str) -> bool {
    // Compare the canonical surface each label resolves to instead of a long
    // synonym OR-chain. When both name a known surface, equality decides; when
    // both are outside the known set (e.g. OpenCode), fall back to a case-
    // insensitive substring match; a known surface never matches an unknown
    // label (else a stray substring like `amp` in `example` would link them).
    match (
        surface_from_text(provider),
        surface_from_text(account_provider),
    ) {
        (Some(left), Some(right)) => left == right,
        (None, None) => {
            let provider = provider.to_ascii_lowercase();
            let account_provider = account_provider.to_ascii_lowercase();
            provider == account_provider
                || provider.contains(&account_provider)
                || account_provider.contains(&provider)
        }
        _ => false,
    }
}

pub(crate) fn most_constrained_fresh_bucket(
    buckets: &[QuotaBucketView],
) -> Option<&QuotaBucketView> {
    // Prefer a rolling-window bucket that actually carries a reset, excluding the
    // monetary Spend slot (already shown as money in the status bar, and it has
    // no rolling reset). Tightest remaining wins; ties break to the soonest reset
    // so the overview row always carries a reset column (Bug 5: a reset-less spend
    // bucket must not win the headline and blank the reset). Fall back to the old
    // "any fresh bucket with a remaining" only when no windowed+reset bucket
    // exists, so a provider that genuinely has only reset-less windows still shows.
    buckets
        .iter()
        .filter(|bucket| bucket.status == UsageSnapshotStatus::Fresh)
        .filter(|bucket| bucket.status_slot != Some(StatusSlot::Spend))
        .filter(|bucket| bucket.remaining_percent.is_some() && bucket.resets_at.is_some())
        // Both keys are `Some` (filtered), so a plain tuple key orders by tightest
        // remaining, then soonest reset.
        .min_by_key(|bucket| (bucket.remaining_percent, bucket.resets_at))
        .or_else(|| {
            buckets
                .iter()
                .filter(|bucket| bucket.status == UsageSnapshotStatus::Fresh)
                .filter(|bucket| bucket.remaining_percent.is_some())
                .min_by_key(|bucket| bucket.remaining_percent.unwrap_or(u8::MAX))
        })
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

/// Present a shared-snapshot view as this instance's last-known **Stale** data:
/// it was fetched by some other instance earlier, not freshly by us. Keeps the
/// numbers, marks the view and its buckets Stale, sources from cache, and sets an
/// "as of" relative label (Class III-C). With Bug 1's marker, the status bar then
/// reads `Updated Xm ago · refreshing...` while this instance's background fetch
/// runs, upgrading to Fresh on completion — never a blank "refreshing" cold start.
pub(crate) fn stale_shared_view(mut view: FocusedUsageView, now: i64) -> FocusedUsageView {
    view.status = UsageSnapshotStatus::Stale;
    view.source = UsageSource::Cache;
    for bucket in &mut view.buckets {
        if bucket.status == UsageSnapshotStatus::Fresh {
            bucket.status = UsageSnapshotStatus::Stale;
        }
    }
    view.updated_label = relative_updated_label(view.fetched_at_epoch, now);
    view
}

pub(crate) fn provider_tabs(active: UsageSurface) -> Vec<UsageProviderTab> {
    [
        UsageSurface::Codex,
        UsageSurface::Claude,
        UsageSurface::Amp,
        UsageSurface::Grok,
        UsageSurface::Zai,
        UsageSurface::Kimi,
        UsageSurface::Minimax,
    ]
    .into_iter()
    .map(|surface| UsageProviderTab {
        label: surface.label().to_owned(),
        status_label: if surface == active { "focused" } else { "" }.to_owned(),
        account_label: "account unavailable".to_owned(),
        plan_label: None,
        source_label: None,
        active: surface == active,
    })
    .collect()
}

pub(crate) fn enrich_provider_tabs(
    view: &mut FocusedUsageView,
    snapshots: &HashMap<String, CachedUsage>,
) {
    let active_label = view.account.provider_label.clone();
    let active_account = compact_account_identity(&view.account.account_label).to_owned();
    let active_plan = view.account.plan_label.clone();
    let active_status = usage_tab_status_label(view);
    let active_source = usage_tab_source_label(view);
    for tab in &mut view.tabs {
        if tab.active || provider_matches_usage_label(&tab.label, &active_label) {
            tab.account_label = active_account.clone();
            tab.plan_label = active_plan.clone();
            tab.status_label = active_status.clone();
            tab.source_label = Some(active_source.clone());
            continue;
        }
        let Some(cached) = snapshots
            .values()
            .filter(|cached| {
                provider_matches_usage_label(&tab.label, &cached.view.account.provider_label)
            })
            .max_by_key(|cached| cached.view.fetched_at_epoch)
        else {
            tab.account_label = "account unavailable".to_owned();
            tab.plan_label = None;
            tab.status_label = "not cached".to_owned();
            tab.source_label = None;
            continue;
        };
        tab.account_label = compact_account_identity(&cached.view.account.account_label).to_owned();
        tab.plan_label = cached.view.account.plan_label.clone();
        tab.status_label = usage_tab_status_label(&cached.view);
        tab.source_label = Some(usage_tab_source_label(&cached.view));
    }
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
        && let Some(bucket) = most_constrained_fresh_bucket(&view.buckets)
        && let Some(remaining) = bucket.remaining_percent
    {
        // A model-scoped window (Fable, Sonnet, …) winning the compact headline
        // is the actionable signal — name it so the Overview/status row tells
        // the operator *which* model is the bottleneck, not just the % left.
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
#[allow(clippy::too_many_arguments)]
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
