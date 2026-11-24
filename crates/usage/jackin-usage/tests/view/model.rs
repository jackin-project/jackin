// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{SecretBytes, Timestamp};
use jackin_testkit::{SeedRow, seed_usage_rows};
use jackin_usage::accounts::{
    AccountPreferences, AccountRegistry, CandidateIdentity, ConnectionError, CredentialVault,
    DATABASE_PATH, FailureKind, UsageProvider,
};
use jackin_usage::dashboard::DashboardSettings;
use jackin_usage::view::card;
use jackin_usage::view::{
    AccountQuota, Attention, Currency, Fact, Note, Reading, ResetStatus, Role, TodayTokens, Unit,
    UsageView, Used, ViewFilter, build,
};
use serde_json::json;
use uuid::Uuid;

use super::{NOW, Result, Seeded};

#[derive(Debug)]
struct Vault;

impl CredentialVault for Vault {
    fn store(&self, _: Uuid, _: &SecretBytes) -> std::result::Result<(), ConnectionError> {
        Ok(())
    }
    fn load(&self, _: Uuid) -> std::result::Result<SecretBytes, ConnectionError> {
        Err(ConnectionError::MissingCredential)
    }
    fn delete(&self, _: Uuid) -> std::result::Result<(), ConnectionError> {
        Ok(())
    }
}

fn find(view: &UsageView, id: Uuid) -> Result<&AccountQuota> {
    view.account(id)
        .map(|(_, account)| account)
        .ok_or_else(|| "the account is missing from the view".into())
}

fn names(meters: &[jackin_usage::view::Meter]) -> Vec<&str> {
    meters.iter().map(|meter| meter.name.as_str()).collect()
}

#[test]
fn main_limit_is_the_binding_overall_window_or_the_scoped_fallback() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = seeded.view(&ViewFilter::default(), NOW)?;
    let expected = [
        ("claude", "work@example.com", 0, "Weekly", false),
        ("claude", "work2@example.com", 0, "Weekly", false),
        ("codex", "work@example.com", 0, "Weekly", false),
        ("muse", "work@example.com", 0, "Weekly", false),
        ("opencode", "work@example.com", 0, "Monthly", false),
        ("zai", "Z.ai Account", 0, "Weekly", false),
        (
            "antigravity",
            "personal@example.com",
            0,
            "Claude weekly",
            false,
        ),
        (
            "antigravity",
            "personal@example.com",
            1,
            "Claude weekly",
            false,
        ),
        ("mimo", "MiMo Token Plan", 0, "Token Plan", false),
        (
            "openrouter",
            "OpenRouter Account",
            0,
            "Free requests",
            false,
        ),
        ("minimax", "MiniMax Token Plan", 0, "General weekly", false),
        ("kimi", "Kimi Code", 0, "Weekly", false),
        ("grok", "Grok Account", 0, "Weekly", true),
    ];
    for (provider, label, nth, name, rolled) in expected {
        let account = find(&view, seeded.id_of(provider, label, nth)?)?;
        let main = account
            .main
            .as_ref()
            .ok_or_else(|| format!("{provider} {label} has no main limit"))?;
        assert_eq!(
            (main.name.as_str(), main.rolled_over),
            (name, rolled),
            "{provider} {label} #{nth}"
        );
    }
    Ok(())
}

#[test]
fn secondary_meters_sort_by_role_then_window_then_name() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = seeded.view(&ViewFilter::default(), NOW)?;
    let secondary = |provider: &str, label: &str, nth: usize| -> Result<Vec<String>> {
        let account = find(&view, seeded.id_of(provider, label, nth)?)?;
        Ok(names(&account.secondary)
            .into_iter()
            .map(str::to_owned)
            .collect())
    };
    assert_eq!(
        secondary("claude", "work2@example.com", 0)?,
        ["Fable weekly", "Extra usage"]
    );
    assert_eq!(
        secondary("opencode", "work@example.com", 0)?,
        ["Session", "Weekly"]
    );
    assert_eq!(
        secondary("antigravity", "personal@example.com", 1)?,
        ["Claude 5h", "Gemini 5h", "Gemini weekly"]
    );
    assert_eq!(
        secondary("zai", "Z.ai Account", 0)?,
        ["Session", "Web searches"]
    );
    let work2 = find(&view, seeded.id_of("claude", "work2@example.com", 0)?)?;
    assert_eq!(
        work2
            .secondary
            .iter()
            .map(|meter| meter.role)
            .collect::<Vec<_>>(),
        [Role::Scoped, Role::Addon]
    );
    Ok(())
}

#[test]
fn a_window_that_reset_after_the_fetch_is_rolled_over() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = seeded.view(&ViewFilter::default(), NOW)?;
    let rolled: Vec<(&str, &str)> = view
        .accounts()
        .flat_map(|(group, account)| {
            account
                .meters()
                .filter(|meter| meter.rolled_over)
                .map(move |meter| (group.name, meter.name.as_str()))
        })
        .collect();
    assert_eq!(rolled.len(), 9);
    let accounts_with_rollover = view
        .accounts()
        .filter(|(_, account)| account.meters().any(|meter| meter.rolled_over))
        .count();
    assert_eq!(accounts_with_rollover, 7);
    assert!(rolled.contains(&("Muse", "Session")));
    assert!(rolled.contains(&("Grok", "Weekly")));

    let muse = seeded.id_of("muse", "work@example.com", 0)?;
    let reset = Timestamp(1_795_493_430);
    let before = seeded.view(&ViewFilter::default(), Timestamp(reset.0 - 1))?;
    let at = seeded.view(&ViewFilter::default(), reset)?;
    let session = |view: &UsageView| -> Result<bool> {
        let account = find(view, muse)?;
        Ok(account
            .meters()
            .find(|meter| meter.name == "Session")
            .ok_or("no session meter")?
            .rolled_over)
    };
    assert!(!session(&before)?);
    assert!(session(&at)?);
    Ok(())
}

#[test]
fn provider_quirks_normalize_to_used_fractions_and_amounts() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = seeded.view(&ViewFilter::default(), NOW)?;

    let antigravity = find(
        &view,
        seeded.id_of("antigravity", "personal@example.com", 0)?,
    )?;
    let weekly = antigravity.main.as_ref().ok_or("no main")?;
    assert!((weekly.fraction().get() - (1.0 - 0.002_806_64)).abs() < 1e-9);
    assert_eq!(weekly.window_secs, Some(604_800));

    let minimax = find(&view, seeded.id_of("minimax", "MiniMax Token Plan", 0)?)?;
    let general = minimax.main.as_ref().ok_or("no main")?;
    assert!(matches!(general.used, Used::Fraction(_)));
    assert!((general.fraction().get() - 0.34).abs() < 1e-9);
    assert_eq!(general.window_secs, Some(604_800));
    assert_eq!(
        minimax
            .secondary
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>(),
        ["General 5h"]
    );
    assert_eq!(minimax.secondary[0].window_secs, Some(18_000));

    let mimo = find(&view, seeded.id_of("mimo", "MiMo Token Plan", 0)?)?;
    let plan = mimo.main.as_ref().ok_or("no main")?;
    match &plan.used {
        Used::Amount { used, limit, unit } => {
            assert!((used - 31_525_748_922.0).abs() < 0.5);
            assert!((limit.get() - 82_000_000_000.0).abs() < 0.5);
            assert_eq!(*unit, Unit::Credits);
        }
        Used::Fraction(_) => return Err("MiMo must keep its credit amounts".into()),
    }

    let claude = find(&view, seeded.id_of("claude", "work2@example.com", 0)?)?;
    let extra = claude
        .secondary
        .iter()
        .find(|meter| meter.name == "Extra usage")
        .ok_or("no extra usage")?;
    let sgd = Currency::new("sgd").ok_or("currency")?;
    assert!(matches!(
        &extra.used,
        Used::Amount { limit, unit, .. } if *unit == Unit::Money(sgd) && (limit.get() - 20.0).abs() < 1e-9
    ));
    assert_eq!(extra.resets_at, None);
    Ok(())
}

#[test]
fn readings_distinguish_fresh_stale_failed_and_report_the_retry() -> Result<()> {
    let seeded = Seeded::new()?;
    let early = seeded.view(&ViewFilter::default(), Timestamp(1_795_484_400))?;
    let kinds: Vec<&str> = early
        .accounts()
        .map(|(_, account)| match account.reading {
            Reading::Fresh { .. } => "fresh",
            Reading::Stale { .. } => "stale",
            Reading::Failed { .. } => "failed",
            Reading::Waiting => "waiting",
        })
        .collect();
    assert_eq!(kinds.iter().filter(|kind| **kind == "fresh").count(), 12);
    assert_eq!(kinds.iter().filter(|kind| **kind == "stale").count(), 1);
    assert_eq!(kinds.iter().filter(|kind| **kind == "failed").count(), 1);

    let view = seeded.view(&ViewFilter::default(), NOW)?;
    let failed = find(
        &view,
        seeded.id_of("antigravity", "personal@example.com", 1)?,
    )?;
    assert_eq!(
        failed.reading,
        Reading::Failed {
            at: Some(Timestamp(1_795_428_364)),
            message: "HTTP 403 SERVICE_DISABLED".to_owned(),
            retry_at: Some(Timestamp(1_795_447_730)),
            reconnect: false,
        }
    );
    assert_eq!(view.stale_ages().len(), 14);
    Ok(())
}

#[test]
fn attention_lists_failures_near_limits_and_duplicates_and_folds_repeats() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = seeded.view(&ViewFilter::default(), NOW)?;
    let first = seeded.id_of("antigravity", "personal@example.com", 0)?;
    let second = seeded.id_of("antigravity", "personal@example.com", 1)?;
    let muse = seeded.id_of("muse", "work@example.com", 0)?;
    assert_eq!(
        view.attention,
        [
            Attention::Failing { account: second },
            Attention::NearLimit {
                accounts: vec![muse],
                meter: "Weekly".to_owned()
            },
            Attention::NearLimit {
                accounts: vec![first, second],
                meter: "Claude weekly".to_owned()
            },
            Attention::Duplicate {
                accounts: vec![first, second],
                remove: second
            },
        ]
    );
    assert_eq!(find(&view, first)?.duplicates, [second]);
    assert_eq!(find(&view, second)?.duplicates, [first]);

    let only_first = seeded.view(
        &ViewFilter {
            provider: None,
            account: Some(first),
        },
        NOW,
    )?;
    assert_eq!(
        only_first.attention,
        [Attention::NearLimit {
            accounts: vec![first],
            meter: "Claude weekly".to_owned()
        }]
    );
    assert!(find(&only_first, first)?.duplicates.is_empty());
    Ok(())
}

#[test]
fn accounts_sharing_a_label_with_different_identities_are_not_duplicates() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let snapshot = json!({"plan": "Google AI Ultra", "resources": {}});
    let rows = ["first", "second"]
        .map(|_| SeedRow {
            provider: UsageProvider::Antigravity,
            label: "shared",
            identity: None,
            fetched_at: NOW.0 - 60,
            snapshot: snapshot.clone(),
            failure: None,
        })
        .into();
    let seeded = seed_usage_rows(&dir.path().join(DATABASE_PATH), rows)?;
    let registry = AccountRegistry::open_read_only(&dir.path().join(DATABASE_PATH))?
        .ok_or("the usage registry is missing")?;
    let view = build(&registry, &ViewFilter::default(), NOW, 0)?;
    for account in &seeded {
        assert!(find(&view, account.id)?.duplicates.is_empty());
    }
    assert!(
        !view
            .attention
            .iter()
            .any(|item| matches!(item, Attention::Duplicate { .. }))
    );
    Ok(())
}

#[test]
fn accounts_with_one_identity_are_duplicates_whatever_their_labels_or_case() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(DATABASE_PATH);
    let mut registry = AccountRegistry::with_vault(&path, Arc::new(Vault))?;
    let secret = SecretBytes::new(vec![1]);
    let first = registry.add(
        UsageProvider::Antigravity,
        "Work",
        &CandidateIdentity::OAuth(Some("Me@Example.com".to_owned())),
        &secret,
    )?;
    let second = registry.add(
        UsageProvider::Antigravity,
        "Home",
        &CandidateIdentity::OAuth(Some("me@example.com".to_owned())),
        &secret,
    )?;
    let other = registry.add(
        UsageProvider::Antigravity,
        "Work",
        &CandidateIdentity::OAuth(Some("other@example.com".to_owned())),
        &secret,
    )?;
    drop(registry);
    let registry =
        AccountRegistry::open_read_only(&path)?.ok_or("the usage registry is missing")?;
    let view = build(&registry, &ViewFilter::default(), NOW, 0)?;
    assert_eq!(find(&view, first)?.duplicates, [second]);
    assert_eq!(find(&view, second)?.duplicates, [first]);
    assert!(find(&view, other)?.duplicates.is_empty());
    Ok(())
}

fn windows(entries: &[(&str, f64, Option<i64>, i64)]) -> serde_json::Value {
    let resources: serde_json::Map<String, serde_json::Value> = entries
        .iter()
        .map(|(key, used, resets_at, window)| {
            let mut resource = json!({
                "kind": "consumption", "unit": "percent", "limit": 100.0,
                "used": used, "window_seconds": window,
            });
            if let Some(at) = resets_at {
                resource["resets_at"] = json!(at);
            }
            ((*key).to_owned(), resource)
        })
        .collect();
    json!({"plan": "Crafted", "resources": resources})
}

type Chosen = (Option<(String, bool)>, Vec<String>);

fn main_of(seeded: &Seeded, body: &serde_json::Value) -> Result<Chosen> {
    let id = seeded.id_of("claude", "work@example.com", 0)?;
    let registry =
        AccountRegistry::with_vault(&seeded.dir.path().join(DATABASE_PATH), Arc::new(Vault))?;
    let version = registry.version(id)?;
    registry.put_snapshot(id, version, NOW.0 - 100, body)?;
    drop(registry);
    let view = seeded.view(&ViewFilter::default(), NOW)?;
    let account = find(&view, id)?;
    Ok((
        account
            .main
            .as_ref()
            .map(|meter| (meter.name.clone(), meter.rolled_over)),
        account
            .secondary
            .iter()
            .map(|meter| meter.name.clone())
            .collect(),
    ))
}

#[test]
fn main_limit_rules_hold_for_crafted_readings() -> Result<()> {
    let seeded = Seeded::new()?;
    let (live, week, five) = (NOW.0 + 3_600, 604_800, 18_000);
    let rolled = NOW.0 - 50;

    let (main, rest) = main_of(
        &seeded,
        &windows(&[
            ("weekly", 2.0, Some(live), week),
            ("fable", 50.0, Some(live), week),
        ]),
    )?;
    assert_eq!(main, Some(("Weekly".to_owned(), false)));
    assert_eq!(rest, ["Fable weekly"]);

    let (main, _) = main_of(
        &seeded,
        &windows(&[
            ("weekly", 2.0, Some(rolled), week),
            ("fable", 50.0, Some(live), week),
        ]),
    )?;
    assert_eq!(main, Some(("Fable weekly".to_owned(), false)));

    let (main, rest) = main_of(
        &seeded,
        &windows(&[
            ("session", 10.0, Some(rolled), five),
            ("weekly", 30.0, Some(rolled), week),
            ("extraUsage", 90.0, Some(live), week),
        ]),
    )?;
    assert_eq!(main, Some(("Weekly".to_owned(), true)));
    assert_eq!(rest, ["Session", "Extra usage"]);

    let (main, rest) = main_of(&seeded, &windows(&[("extraUsage", 90.0, Some(live), week)]))?;
    assert_eq!(main, None);
    assert_eq!(rest, ["Extra usage"]);

    let (main, _) = main_of(
        &seeded,
        &windows(&[
            ("session", 50.0, Some(live), five),
            ("weekly", 50.0, Some(live), week),
        ]),
    )?;
    assert_eq!(main, Some(("Weekly".to_owned(), false)));

    let (main, _) = main_of(
        &seeded,
        &windows(&[
            ("session", 50.2, Some(live), five),
            ("weekly", 49.6, Some(live), week),
        ]),
    )?;
    assert_eq!(main, Some(("Weekly".to_owned(), false)));

    let (main, _) = main_of(
        &seeded,
        &windows(&[
            ("session", 60.0, None, five),
            ("weekly", 10.0, Some(live), week),
        ]),
    )?;
    assert_eq!(main, Some(("Session".to_owned(), false)));
    Ok(())
}

#[test]
fn notes_and_facts_carry_credits_resets_tokens_spend_and_account_facts() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = seeded.view(&ViewFilter::default(), NOW)?;

    let codex = find(&view, seeded.id_of("codex", "work@example.com", 0)?)?;
    assert!(matches!(
        &codex.notes[0],
        Note::Balance { amount, unit: Unit::Credits, approx: Some((usd, currency)), .. }
            if (amount - 19_290.0).abs() < 1e-9 && (usd - 771.6).abs() < 1e-9 && currency.as_str() == "USD"
    ));
    assert!(
        matches!(&codex.notes[1], Note::Resets(credits) if credits.len() == 1 && credits[0].status == ResetStatus::Ready)
    );
    match &codex.notes[2] {
        Note::Tokens {
            today: TodayTokens::Unavailable,
            yesterday: Some(yesterday),
            last30: Some(last30),
        } => {
            assert_eq!(yesterday.tokens, 420_307);
            assert_eq!(yesterday.cached_in, 354_176);
            assert_eq!(last30.tokens, 96_689_333_883);
            assert_eq!(last30.observed, (30, 30));
            assert_eq!(
                last30.models.first(),
                Some(&("gpt-6.1-terra".to_owned(), 2_484))
            );
            assert_eq!(last30.models.len(), 8);
        }
        other => return Err(format!("unexpected tokens note {other:?}").into()),
    }

    let zai = find(&view, seeded.id_of("zai", "Z.ai Account", 0)?)?;
    match zai.notes.first() {
        Some(Note::Resets(credits)) => {
            let status = |wanted| {
                credits
                    .iter()
                    .filter(|credit| credit.status == wanted)
                    .count()
            };
            assert_eq!(status(ResetStatus::Ready), 1);
            assert_eq!(status(ResetStatus::Queued), 1);
            assert_eq!(credits.len(), 2);
        }
        other => return Err(format!("unexpected reset note {other:?}").into()),
    }

    let openrouter = find(&view, seeded.id_of("openrouter", "OpenRouter Account", 0)?)?;
    assert!(matches!(
        openrouter.notes.last(),
        Some(Note::Spend { today, week, month, byok, currency })
            if [*today, *week, *month].iter().all(|amount| amount.is_some_and(|value| value.abs() < 1e-12)) && *byok == Some(0.0) && *currency == Currency::USD
    ));
    assert_eq!(openrouter.facts, [Fact::KeyType("standard".to_owned())]);

    let mimo = find(&view, seeded.id_of("mimo", "MiMo Token Plan", 0)?)?;
    assert_eq!(
        mimo.facts,
        [
            Fact::Region("sgp".to_owned()),
            Fact::AutoRenew(true),
            Fact::MimoClaw(false),
            Fact::ApiKeyAuth,
            Fact::PeriodStart(Timestamp(1_794_407_234)),
        ]
    );
    let google = find(
        &view,
        seeded.id_of("antigravity", "personal@example.com", 1)?,
    )?;
    assert_eq!(
        google.facts,
        [Fact::GoogleProject("example-project".to_owned())]
    );
    Ok(())
}

#[test]
fn dashboard_settings_drive_order_names_and_thresholds() -> Result<()> {
    let seeded = Seeded::new()?;
    let default = seeded.view(&ViewFilter::default(), NOW)?;
    assert_eq!(
        default
            .groups
            .iter()
            .map(|group| group.name)
            .collect::<Vec<_>>(),
        [
            "Claude",
            "ChatGPT / Codex",
            "Muse",
            "Grok",
            "Kimi",
            "Z.AI",
            "MiniMax",
            "MiMo",
            "Antigravity (Google AI)",
            "OpenCode",
            "OpenRouter"
        ]
    );
    assert!((default.warn_at.get() - 0.7).abs() < 1e-9);
    assert!((default.critical_at.get() - 0.9).abs() < 1e-9);

    let registry =
        AccountRegistry::with_vault(&seeded.dir.path().join(DATABASE_PATH), Arc::new(Vault))?;
    registry.set_dashboard_settings(&DashboardSettings {
        provider_order: vec![UsageProvider::OpenRouter, UsageProvider::Mimo],
        conceal_names: true,
        warning: 40,
        critical: 20,
        ..DashboardSettings::default()
    })?;
    drop(registry);
    let view = seeded.view(&ViewFilter::default(), NOW)?;
    assert_eq!(
        view.groups
            .iter()
            .take(4)
            .map(|group| group.name)
            .collect::<Vec<_>>(),
        ["OpenRouter", "MiMo", "Claude", "ChatGPT / Codex"]
    );
    assert!((view.warn_at.get() - 0.6).abs() < 1e-9);
    assert!((view.critical_at.get() - 0.8).abs() < 1e-9);
    let labels: Vec<&str> = view
        .accounts()
        .map(|(_, account)| account.label.as_str())
        .collect();
    assert!(labels.iter().all(|label| label.starts_with("Account ")));
    assert!(!labels.iter().any(|label| label.contains('@')));
    let failed = view
        .accounts()
        .find_map(|(_, account)| match &account.reading {
            Reading::Failed { message, .. } => Some(message.as_str()),
            _ => None,
        })
        .ok_or("no failed account")?;
    assert_eq!(failed, "Current usage unavailable.");
    Ok(())
}

#[test]
fn metrics_the_user_hid_stay_out_of_the_view() -> Result<()> {
    let seeded = Seeded::new()?;
    let id = seeded.id_of("claude", "work@example.com", 0)?;
    let registry =
        AccountRegistry::with_vault(&seeded.dir.path().join(DATABASE_PATH), Arc::new(Vault))?;
    registry.set_preferences(
        id,
        &AccountPreferences {
            hidden: ["weekly".to_owned(), "rateLimitResets".to_owned()].into(),
            ..AccountPreferences::default()
        },
    )?;
    drop(registry);
    let view = seeded.view(&ViewFilter::default(), NOW)?;
    let account = find(&view, id)?;
    assert_eq!(
        account.main.as_ref().map(|meter| meter.name.as_str()),
        Some("Fable weekly")
    );
    assert!(account.secondary.is_empty());
    assert!(account.notes.is_empty());
    Ok(())
}

#[test]
fn the_duplicate_to_remove_is_the_failing_one_else_the_last_connected() -> Result<()> {
    let seeded = Seeded::new()?;
    let first = seeded.id_of("antigravity", "personal@example.com", 0)?;
    let second = seeded.id_of("antigravity", "personal@example.com", 1)?;
    let registry =
        AccountRegistry::with_vault(&seeded.dir.path().join(DATABASE_PATH), Arc::new(Vault))?;
    let body = &seeded.accounts[13].snapshot;
    registry.put_snapshot(second, registry.version(second)?, NOW.0 - 100, body)?;
    drop(registry);
    let removal = |view: &UsageView| {
        view.attention.iter().find_map(|item| match item {
            Attention::Duplicate { remove, .. } => Some(*remove),
            Attention::Failing { .. } | Attention::NearLimit { .. } => None,
        })
    };
    let healthy = seeded.view(&ViewFilter::default(), NOW)?;
    assert_eq!(removal(&healthy), Some(second));
    assert!(
        !healthy
            .attention
            .iter()
            .any(|item| matches!(item, Attention::Failing { .. }))
    );

    let mut registry =
        AccountRegistry::with_vault(&seeded.dir.path().join(DATABASE_PATH), Arc::new(Vault))?;
    let version = registry.version(first)?;
    registry.record_failure(
        first,
        version,
        NOW.0 - 50,
        FailureKind::Network,
        None,
        "offline",
    )?;
    drop(registry);
    let failing = seeded.view(&ViewFilter::default(), NOW)?;
    assert_eq!(removal(&failing), Some(first));
    assert_eq!(failing.attention[0], Attention::Failing { account: first });
    Ok(())
}

fn crafted_account(
    provider: UsageProvider,
    snapshot: serde_json::Value,
    hidden: &[&str],
) -> Result<AccountQuota> {
    let (view, id) = crafted_view(provider, snapshot, hidden)?;
    Ok(find(&view, id)?.clone())
}

fn crafted_view(
    provider: UsageProvider,
    snapshot: serde_json::Value,
    hidden: &[&str],
) -> Result<(UsageView, Uuid)> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(DATABASE_PATH);
    let seeded = seed_usage_rows(
        &path,
        vec![SeedRow {
            provider,
            label: "Account",
            identity: None,
            fetched_at: NOW.0 - 60,
            snapshot,
            failure: None,
        }],
    )?;
    let id = seeded[0].id;
    let registry = AccountRegistry::with_vault(&path, Arc::new(Vault))?;
    registry.set_preferences(
        id,
        &AccountPreferences {
            hidden: hidden.iter().map(|key| (*key).to_owned()).collect(),
            ..AccountPreferences::default()
        },
    )?;
    drop(registry);
    let registry =
        AccountRegistry::open_read_only(&path)?.ok_or("the usage registry is missing")?;
    let view = build(&registry, &ViewFilter::default(), NOW, 0)?;
    Ok((view, id))
}

fn token_ranges() -> serde_json::Value {
    let range = |tokens: u64| json!({"totals": {"tokens": tokens, "turns": 1}});
    json!({"plan": "Plus", "resources": {}, "history": {"ranges": {
        "today": range(10), "yesterday": range(20), "last30": range(30),
    }}})
}

fn spend_periods() -> serde_json::Value {
    json!({"plan": "Pay", "resources": {}, "period_spend": {
        "today": {"spend": 1.0},
        "thisWeek": {"spend": 2.0},
        "thisMonth": {"spend": 3.0, "byokSpend": 0.5},
    }})
}

fn tokens_of(account: &AccountQuota) -> Option<(Option<u64>, Option<u64>, Option<u64>)> {
    account.notes.iter().find_map(|note| match note {
        Note::Tokens {
            today,
            yesterday,
            last30,
        } => Some((
            match today {
                TodayTokens::Count(tokens) => Some(*tokens),
                TodayTokens::Hidden | TodayTokens::Unavailable => None,
            },
            yesterday.as_ref().map(|activity| activity.tokens),
            last30.as_ref().map(|activity| activity.tokens),
        )),
        _ => None,
    })
}

type Periods = (Option<f64>, Option<f64>, Option<f64>, Option<f64>);

fn spend_of(account: &AccountQuota) -> Option<Periods> {
    account.notes.iter().find_map(|note| match note {
        Note::Spend {
            today,
            week,
            month,
            byok,
            ..
        } => Some((*today, *week, *month, *byok)),
        _ => None,
    })
}

#[test]
fn hidden_history_and_spend_periods_stay_out_of_the_notes() -> Result<()> {
    let tokens = |hidden: &[&str]| -> Result<_> {
        Ok(tokens_of(&crafted_account(
            UsageProvider::ChatGpt,
            token_ranges(),
            hidden,
        )?))
    };
    assert_eq!(tokens(&[])?, Some((Some(10), Some(20), Some(30))));
    assert_eq!(tokens(&["today", "last30"])?, Some((None, Some(20), None)));
    assert_eq!(tokens(&["today", "yesterday", "last30"])?, None);

    let spend = |hidden: &[&str]| -> Result<_> {
        Ok(spend_of(&crafted_account(
            UsageProvider::OpenRouter,
            spend_periods(),
            hidden,
        )?))
    };
    assert_eq!(
        spend(&[])?,
        Some((Some(1.0), Some(2.0), Some(3.0), Some(0.5)))
    );
    assert_eq!(
        spend(&["week", "month"])?,
        Some((Some(1.0), None, None, None))
    );
    assert_eq!(spend(&["today", "week", "month"])?, None);

    let legacy = json!({"plan": "Pay", "resources": {}, "activity": {
        "yesterday": {"totals": {"tokens": 7}},
        "prior30Days": {"tokens": 70},
        "observed_days": 30, "expected_days": 30,
    }});
    let legacy_tokens = |hidden: &[&str]| -> Result<_> {
        Ok(tokens_of(&crafted_account(
            UsageProvider::OpenRouter,
            legacy.clone(),
            hidden,
        )?))
    };
    assert_eq!(legacy_tokens(&[])?, Some((None, Some(7), Some(70))));
    assert_eq!(legacy_tokens(&["prior30"])?, Some((None, Some(7), None)));
    assert_eq!(legacy_tokens(&["prior30", "yesterday"])?, None);
    Ok(())
}

#[test]
fn an_idle_antigravity_five_hour_window_has_no_reset_but_a_used_one_keeps_it() -> Result<()> {
    let window = |used: f64| {
        json!({
            "kind": "quota", "unit": "percent",
            "remaining_fraction": 1.0 - used,
            "resets_at": NOW.0 + 3_600, "window_seconds": 18_000,
        })
    };
    let reset_of = |account: &AccountQuota, key: &str| -> Result<Option<Timestamp>> {
        account
            .meters()
            .find(|meter| meter.key == key)
            .map(|meter| meter.resets_at)
            .ok_or_else(|| format!("no {key} meter").into())
    };
    let idle = crafted_account(
        UsageProvider::Antigravity,
        json!({"plan": "Ultra", "resources": {
            "gemini-5h": window(0.0), "3p-5h": window(0.0), "gemini-weekly": window(0.0),
        }}),
        &[],
    )?;
    assert_eq!(reset_of(&idle, "gemini-5h")?, None);
    assert_eq!(reset_of(&idle, "3p-5h")?, None);
    assert_eq!(
        reset_of(&idle, "gemini-weekly")?,
        Some(Timestamp(NOW.0 + 3_600))
    );

    let used = crafted_account(
        UsageProvider::Antigravity,
        json!({"plan": "Ultra", "resources": {"gemini-5h": window(0.25)}}),
        &[],
    )?;
    assert_eq!(
        reset_of(&used, "gemini-5h")?,
        Some(Timestamp(NOW.0 + 3_600))
    );

    let other = crafted_account(
        UsageProvider::Claude,
        json!({"plan": "Max", "resources": {"gemini-5h": window(0.0)}}),
        &[],
    )?;
    assert_eq!(
        reset_of(&other, "gemini-5h")?,
        Some(Timestamp(NOW.0 + 3_600))
    );
    Ok(())
}

fn token_parts(snapshot: serde_json::Value, hidden: &[&str]) -> Result<Option<Vec<String>>> {
    let (view, id) = crafted_view(UsageProvider::ChatGpt, snapshot, hidden)?;
    Ok(find(&view, id)?
        .notes
        .iter()
        .find(|note| matches!(note, Note::Tokens { .. }))
        .map(|note| card::note_parts(note, &view).1))
}

#[test]
fn a_hidden_today_is_omitted_and_an_unreported_one_reads_n_a() -> Result<()> {
    assert_eq!(
        token_parts(token_ranges(), &[])?,
        Some(vec![
            "today 10".to_owned(),
            "yesterday 20".to_owned(),
            "30 days 30".to_owned()
        ])
    );
    assert_eq!(
        token_parts(token_ranges(), &["today"])?,
        Some(vec!["yesterday 20".to_owned(), "30 days 30".to_owned()])
    );
    let legacy = json!({"plan": "Pay", "resources": {}, "activity": {
        "yesterday": {"totals": {"tokens": 7}},
        "prior30Days": {"tokens": 70},
        "observed_days": 30, "expected_days": 30,
    }});
    assert_eq!(
        token_parts(legacy, &[])?,
        Some(vec![
            "today n/a".to_owned(),
            "yesterday 7".to_owned(),
            "30 days 70".to_owned()
        ])
    );
    Ok(())
}
