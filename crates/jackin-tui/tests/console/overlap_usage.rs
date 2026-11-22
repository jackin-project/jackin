// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::owned_usage::{Expected, journey};
use jackin_core::{Provider, Secret, Timestamp};
use jackin_testkit::{Method, Route, TestResult, UsageServer, guard};
use jackin_usage::{accounts::UsageProvider as U, connected::KeyConnection, quota};
use serde_json::{Value, json};

async fn check(
    provider: U,
    public: Provider,
    routes: &[(Method, &str, Value)],
    expected: Expected<'_>,
    metric: Option<(&str, &[&str])>,
) -> TestResult {
    guard(async {
        let server = UsageServer::start().await?;
        for (method, path, body) in routes {
            server.serve(
                public,
                *method,
                path,
                Route::json(200, &body.to_string())
                    .requires_header("authorization", "Bearer synthetic-access"),
            );
        }
        let key = Secret::new("synthetic-access".into());
        let now = Timestamp(1767225600);
        let snapshot = if matches!(provider, U::Zai | U::OpenRouter) {
            KeyConnection::for_provider(public, None)?
                .read_with_policy(&server.endpoints(), &key, now, None, false)
                .await?
        } else {
            let http = server.endpoints().http(public)?;
            quota::read(provider, &http, &key, Some("work"), now, None).await?
        };
        assert!(server.mismatches().is_empty(), "{:?}", server.mismatches());
        journey(provider, snapshot, expected, metric).await
    })
    .await
}

#[tokio::test]
async fn claude_currency_and_reset_inventory_reach_the_account() -> TestResult {
    check(
        U::Claude,
        Provider::Claude,
        &[
            (
                Method::Get,
                "/api/oauth/usage",
                json!({
                    "extra_usage": {
                        "is_enabled": true,
                        "currency": "EUR",
                        "decimal_places": 2,
                        "used_credits": 1234,
                        "monthly_limit": 5000
                    }
                }),
            ),
            (
                Method::Get,
                "/api/oauth/usage?cedar_ember=1&skip_spend=1",
                json!({
                    "cedar_ember": {
                        "eligible": true,
                        "grants": [
                            {
                                "id": "private-reset-id",
                                "resets_left": 2,
                                "ends_at": 1893456000,
                                "type": "full",
                                "usable_now": true
                            }
                        ]
                    }
                }),
            ),
            (
                Method::Get,
                "/api/oauth/profile",
                json!({
                    "organization": {
                        "organization_type": "claude_enterprise"
                    }
                }),
            ),
        ],
        Expected {
            account: &["Enterprise", "Spend", "12.34 eur", "50 eur", "Limit resets"],
            board: &["Spend", "Limit resets"],
            absent: &["private-reset-id", "synthetic-access"],
        },
        Some(("Spend", &["12.34 eur", "50 eur"])),
    )
    .await
}

#[tokio::test]
async fn codex_model_history_survives_the_account_and_board() -> TestResult {
    check(U::ChatGpt,
        Provider::Codex,&[
        (Method::Get,"/backend-api/wham/usage",json!({
    "plan_type": "plus",
    "rate_limit": {
        "primary_window": {
            "limit_window_seconds": 604800,
            "used_percent": 42
        }
    },
    "rate_limit_reset_credits": {
        "available_count": 0
    }
})),
        (Method::Get,"/backend-api/wham/rate-limit-reset-credits",json!({"available_count": 0, "credits": []})),
        (Method::Get,"/backend-api/wham/analytics/daily-workspace-usage-counts?start_date=2025-12-03&end_date=2026-01-01&group_by=day&workspace_user=true",json!({
    "data": [
        {
            "date": "2026-01-01",
            "totals": {
                "turns": 2,
                "text_total_tokens": 71
            },
            "models": [
                {
                    "model": "model-fixture",
                    "turns": 2
                }
            ]
        }
    ]
}))
    ],Expected {
            account: &["Weekly","Today","71 tokens","model-fixture"],
            board: &["Weekly","Today","71 tokens"],
            absent: &["synthetic-access"]
        },
        Some(("Today",&["model-fixture","2 turns"])),
    ).await
}

#[tokio::test]
async fn muse_active_session_and_inactive_notice_remain_distinct() -> TestResult {
    check(
        U::Muse,
        Provider::Muse,
        &[(
            Method::Post,
            "/muse-code/key",
            json!({
                "is_subs_active": true,
                "subs_tier_name": "Muse Plus",
                "api_key": "private-muse",
                "subs_usage": {
                    "window": {
                        "used_percent": 19,
                        "resets_at": 1893456000
                    },
                    "weekly": {
                        "used_percent": 7,
                        "resets_at": 1893456000
                    }
                }
            }),
        )],
        Expected {
            account: &["Muse Plus", "Session", "19%", "Weekly"],
            board: &["Session", "Weekly"],
            absent: &["private-muse"],
        },
        Some(("Session", &["Muse Plus", "19%"])),
    )
    .await?;
    check(
        U::Muse,
        Provider::Muse,
        &[(
            Method::Post,
            "/muse-code/key",
            json!({"is_subs_active": false}),
        )],
        Expected {
            account: &["No Muse Code subscription"],
            board: &["No Muse Code subscription"],
            absent: &["0%"],
        },
        None,
    )
    .await
}

#[tokio::test]
async fn grok_uncapped_extra_usage_keeps_its_currency() -> TestResult {
    check(
        U::Grok,
        Provider::Grok,
        &[
            (
                Method::Get,
                "/v1/billing?format=credits",
                json!({
                    "config": {
                        "currentPeriod": {
                            "type": "USAGE_PERIOD_TYPE_WEEKLY",
                            "start": "2026-01-01T00:00:00Z",
                            "end": "2026-01-08T00:00:00Z"
                        },
                        "creditUsagePercent": 15,
                        "onDemandCap": {
                            "val": "150"
                        }
                    }
                }),
            ),
            (
                Method::Get,
                "/v1/settings",
                json!({"subscription_tier_display": "Grok Heavy"}),
            ),
        ],
        Expected {
            account: &["Grok Heavy", "Weekly", "Extra usage cap", "1.5 usd"],
            board: &["Extra usage cap", "1.5 usd"],
            absent: &["synthetic-access"],
        },
        Some(("Extra usage cap", &["1.5 usd"])),
    )
    .await
}

#[tokio::test]
async fn opencode_org_windows_preserve_reported_zero() -> TestResult {
    check(
        U::OpenCode,
        Provider::Opencode,
        &[
            (
                Method::Get,
                "/zen/go/v1/usage",
                json!({
                    "usage": {
                        "rolling": {
                            "percent": 0,
                            "resetsAt": "2026-01-01T05:00:00Z"
                        },
                        "weekly": {
                            "percent": 25,
                            "resetsAt": "2026-01-08T00:00:00Z"
                        },
                        "monthly": {
                            "percent": 46,
                            "resetsAt": "2026-02-01T00:00:00Z"
                        }
                    }
                }),
            ),
            (
                Method::Get,
                "/console/api/go/status",
                json!({"product": "go-plus"}),
            ),
        ],
        Expected {
            account: &["Go Plus", "Session", "0%", "Monthly", "46%"],
            board: &["Session", "Monthly"],
            absent: &["synthetic-access"],
        },
        Some(("Monthly", &["Go Plus", "46%"])),
    )
    .await
}

#[tokio::test]
async fn zai_token_limit_and_search_allowance_are_distinct() -> TestResult {
    check(
        U::Zai,
        Provider::Zai,
        &[
            (
                Method::Get,
                "/api/monitor/usage/quota/limit",
                json!({
                    "data": {
                        "limits": [
                            {
                                "type": "TOKENS_LIMIT",
                                "unit": 3,
                                "number": 5,
                                "percentage": 28
                            },
                            {
                                "type": "TIME_LIMIT",
                                "currentValue": 3,
                                "usage": 100
                            }
                        ]
                    }
                }),
            ),
            (
                Method::Get,
                "/api/biz/subscription/list",
                json!({"data": [{"productName": "GLM Pro"}]}),
            ),
            (
                Method::Get,
                "/api/biz/customer-package-reset/list?targetType=PERSONAL",
                json!({"data": {}}),
            ),
        ],
        Expected {
            account: &["GLM Pro", "Web searches", "3 searches", "100 searches"],
            board: &["Session", "Web searches"],
            absent: &["synthetic-access"],
        },
        Some(("Web searches", &["3 searches", "100 searches"])),
    )
    .await
}

#[tokio::test]
async fn openrouter_separates_period_spend_and_management_activity() -> TestResult {
    check(
        U::OpenRouter,
        Provider::Openrouter,
        &[
            (
                Method::Get,
                "/api/v1/key",
                json!({"data": {"is_management_key": true}}),
            ),
            (
                Method::Get,
                "/api/v1/credits",
                json!({
                    "data": {
                        "total_credits": 30.0,
                        "total_usage": 10.0
                    }
                }),
            ),
            (
                Method::Get,
                "/api/v1/keys?include_disabled=true&include_expired=true&offset=0",
                json!({
                    "data": [
                        {
                            "usage_daily": 3.25,
                            "usage_weekly": 9,
                            "usage_monthly": 11,
                            "byok_usage_daily": 0.5
                        }
                    ]
                }),
            ),
            (
                Method::Get,
                "/api/v1/activity",
                json!({
                    "data": [
                        {
                            "date": "2025-12-31",
                            "model": "router-model",
                            "usage": 7.5,
                            "byok_usage_inference": 1,
                            "prompt_tokens": 13,
                            "completion_tokens": 17
                        }
                    ]
                }),
            ),
        ],
        Expected {
            account: &[
                "Today",
                "3.25 usd",
                "This week",
                "9 usd",
                "Yesterday",
                "7.5 usd",
                "router-model",
            ],
            board: &["3.25 usd", "Prior 30 days", "7.5 usd"],
            absent: &["synthetic-access"],
        },
        Some((
            "Prior 30 days",
            &[
                "router-model",
                "30 tokens",
                "13 input tokens",
                "17 output tokens",
                "1 USD BYOK spend",
                "30 of 30 days observed",
            ],
        )),
    )
    .await?;
    check(
        U::OpenRouter,
        Provider::Openrouter,
        &[(
            Method::Get,
            "/api/v1/key",
            json!({"data":{
                "is_management_key":false,"is_free_tier":true,
                "usage_daily":0,"usage_weekly":1.25,"usage_monthly":2.5
            }}),
        )],
        Expected {
            account: &["Free", "Today", "0 usd", "This week", "1.25 usd"],
            board: &["Today", "0 usd", "This month", "2.5 usd"],
            absent: &["Prior 30 days", "Yesterday", "BYOK spend"],
        },
        Some(("Today", &["0 USD spent"])),
    )
    .await
}
