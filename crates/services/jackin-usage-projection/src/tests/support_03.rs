// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn parity_partial_account() -> ParityAccount {
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

pub(super) fn parity_unsupported_account() -> ParityAccount {
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

pub(super) fn parity_auth_account() -> ParityAccount {
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

pub(super) fn parity_error_account() -> ParityAccount {
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

pub(super) fn parity_zero_account() -> ParityAccount {
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

pub(super) fn parity_old_stale_account() -> ParityAccount {
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

pub(super) fn parity_mega_providers() -> Vec<ParityProvider> {
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

pub(super) fn parity_unresolved_entries() -> Vec<UsageUnresolvedV1> {
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

pub(super) fn parity_projection_issue() -> UsageIssueV1 {
    parity_issue(
        "broker_degraded",
        UsageIssueScopeV1::Projection,
        UsageIssueRecoverabilityV1::Retryable,
        "one provider refresh failed",
        None,
    )
}

pub(super) fn parity_single_provider(
    provider: ParityProvider,
) -> (UsageProjectionV1, Vec<FocusedUsageView>) {
    let (projection, views) = parity_projection(&[provider], Vec::new(), Vec::new());
    (projection, views.into_iter().next().unwrap())
}

pub(super) fn parity_render_text(
    screen_state: UsageScreenState,
    width: u16,
    height: u16,
    render_at: i64,
) -> String {
    use jackin_console::tui::state::ManagerState;
    use ratatui::{Terminal, backend::TestBackend};

    let config = jackin_config::AppConfig::default();
    let mut manager = ManagerState::from_config(&config, std::path::Path::new("/test"));
    manager.usage.screen = Some(screen_state);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            jackin_console::tui::screens::usage::render_at(
                frame,
                frame.area(),
                &manager,
                render_at,
            );
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
