// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn first_credential_uses_home_first_then_handoff_fallback() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home.credentials.json");
    let handoff = dir.path().join("handoff.credentials.json");

    fs::write(&home, r#"{"oauthAccount":{"emailAddress":"a@b.c"}}"#).expect("write home");
    fs::write(
        &handoff,
        r#"{"claudeAiOauth":{"accessToken":"handoff-token"}}"#,
    )
    .expect("write handoff");
    let resolved = first_credential(
        &[home.clone(), handoff.clone()],
        load_claude_oauth_credentials,
    );
    assert!(
        resolved
            .as_ref()
            .is_some_and(|c| { c.access_token.as_str() == "handoff-token" })
    );

    fs::write(&home, r#"{"claudeAiOauth":{"accessToken":"home-token"}}"#).expect("rewrite home");
    let resolved = first_credential(&[home, handoff], load_claude_oauth_credentials);
    assert!(
        resolved
            .as_ref()
            .is_some_and(|c| { c.access_token.as_str() == "home-token" })
    );
}

#[test]
fn codex_rpc_maps_spark_windows_and_reset_credits() {
    let body = r#"{
            "rateLimits": {"limitId": "codex",
                "primary": {"usedPercent": 7, "windowDurationMins": 300, "resetsAt": 1782396144},
                "secondary": {"usedPercent": 5, "windowDurationMins": 10080, "resetsAt": 1782940724},
                "credits": {"hasCredits": false, "unlimited": false, "balance": "0"},
                "planType": "pro"},
            "rateLimitsByLimitId": {
                "codex_bengalfox": {"limitId": "codex_bengalfox", "limitName": "GPT-5.3-Codex-Spark",
                    "primary": {"usedPercent": 0, "windowDurationMins": 300, "resetsAt": 1782411283},
                    "secondary": {"usedPercent": 0, "windowDurationMins": 10080, "resetsAt": 1782998083}},
                "codex": {"limitId": "codex",
                    "primary": {"usedPercent": 7, "windowDurationMins": 300, "resetsAt": 1782396144},
                    "secondary": {"usedPercent": 5, "windowDurationMins": 10080, "resetsAt": 1782940724}}
            },
            "rateLimitResetCredits": {"availableCount": 2}
        }"#;
    let limits: CodexRpcRateLimitsResponse =
        serde_json::from_str(body).expect("decode rateLimits response");
    let usage = CodexRpcUsage::from_rpc(limits, None);
    let labels: Vec<String> = usage
        .response
        .buckets(1_782_300_000)
        .into_iter()
        .map(|b| b.label)
        .collect();
    assert!(labels.contains(&"Session".to_owned()));
    assert!(labels.contains(&"Weekly".to_owned()));
    assert!(labels.contains(&"Codex Spark 5-hour".to_owned()));
    assert!(labels.contains(&"Codex Spark Weekly".to_owned()));
    assert!(labels.contains(&"Limit Reset Credits".to_owned()));

    assert_eq!(labels.iter().filter(|l| l.as_str() == "Session").count(), 1);
}
