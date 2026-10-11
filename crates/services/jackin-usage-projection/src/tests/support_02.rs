// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn parity_antigravity_account() -> ParityAccount {
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

pub(super) fn parity_claude_work_account() -> ParityAccount {
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

pub(super) fn parity_claude_personal_account() -> ParityAccount {
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

pub(super) fn parity_cursor_account() -> ParityAccount {
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

pub(super) fn parity_exhausted_account() -> ParityAccount {
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
