// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_gemini::antigravity_statusline;
use serde_json::json;
use std::error::Error;

#[test]
fn distinguishes_all_antigravity_windows_and_keeps_only_account_usage() -> Result<(), Box<dyn Error>>
{
    let payload = json!({
        "product": "antigravity", "email": "tester@example.test", "plan_tier": "Ultra",
        "cwd": "/private/work", "conversation_title": "private conversation",
        "quota": {
            "gemini-5h":{"remaining_fraction":1.0,"reset_time":"2024-01-01T00:00:00Z","reset_in_seconds":3600},
            "gemini-weekly":{"remaining_fraction":0.9971966,"reset_in_seconds":600},
            "3p-5h":{"remaining_fraction":0.0,"reset_time":"unknown","reset_in_seconds":60},
            "3p-weekly":{"remaining_fraction":0.00280664,"reset_time":"unknown"},
            "gemini-argon-wtu-weekly":{"remaining_fraction":0.98,"disabled":true},
            "future":{"remaining_amount":50}
        }
    });
    let reading =
        antigravity_statusline(&serde_json::to_vec(&payload)?, 100)?.ok_or("missing reading")?;
    assert_eq!(reading.email, "tester@example.test");
    assert_eq!(reading.snapshot["plan"], "Google AI Ultra");
    let resources = reading.snapshot["resources"]
        .as_object()
        .ok_or("missing resources")?;
    assert_eq!(resources.len(), 4);
    for (id, remaining, seconds) in [
        ("gemini-5h", 1., 18000),
        ("gemini-weekly", 0.9971966, 604800),
        ("3p-5h", 0., 18000),
        ("3p-weekly", 0.00280664, 604800),
    ] {
        let row = &resources[id];
        assert_eq!(row["remaining_fraction"], remaining);
        assert_eq!(row["window_seconds"], seconds);
        assert_eq!(row["unit"], "percent");
    }
    assert_eq!(resources["gemini-5h"]["resets_at"], 1704067200);
    assert_eq!(resources["gemini-weekly"]["resets_at"], 700);
    assert_eq!(resources["3p-5h"]["resets_at"], 160);
    assert!(resources["3p-weekly"].get("resets_at").is_none());
    assert!(!reading.snapshot.to_string().contains("private"));
    Ok(())
}

#[test]
fn rejects_unusable_readings_and_preserves_omitted_values() -> Result<(), Box<dyn Error>> {
    let mut payload = json!({"product":"antigravity","email":"a@example.test","quota":{"gemini-weekly":{"remaining_fraction":0.6}}});
    let parse = |v: &serde_json::Value| {
        antigravity_statusline(&serde_json::to_vec(v)?, 1).map_err(Box::<dyn Error>::from)
    };
    let partial = parse(&payload)?.ok_or("missing partial reading")?;
    assert!(partial.snapshot.get("plan").is_none());
    assert!(partial.snapshot["resources"].get("3p-weekly").is_none());
    payload["product"] = json!("gemini");
    parse(&payload)
        .err()
        .ok_or("expected invalid reading to fail")?;
    payload["product"] = json!("antigravity");
    for quota in [
        json!({}),
        json!({"gemini-weekly":{"remaining_amount":-1}}),
        json!({"gemini-weekly":{"remaining_amount":"5"}}),
        json!({"gemini-weekly":{"remaining_amount":5,"disabled":true}}),
        json!({"gemini-weekly":{"remaining_fraction":0.5,"disabled":"false"}}),
    ] {
        payload["quota"] = quota;
        assert!(parse(&payload)?.is_none());
    }
    payload["quota"] = json!([]);
    parse(&payload)
        .err()
        .ok_or("expected malformed quota object to fail")?;
    let mixed_input = br#"{
        "product":"antigravity", "email":"a@example.test",
        "quota":{
            "gemini-5h":{"remaining_fraction":1.1,"reset_time":{},"reset_in_seconds":10},
            "gemini-weekly":{"remaining_fraction":-0.1,"reset_in_seconds":"unknown"},
            "gemini-argon-wtu-weekly":{"remaining_fraction":0.4},
            "3p-5h":{"remaining_fraction":"invalid"},
            "3p-weekly":{"remaining_fraction":1e400}
        }
    }"#;
    let mixed = antigravity_statusline(mixed_input, 1)?.ok_or("missing mixed reading")?;
    let resources = mixed.snapshot["resources"]
        .as_object()
        .ok_or("missing resources")?;
    assert_eq!(resources.len(), 3);
    assert_eq!(resources["gemini-5h"]["remaining_fraction"], 1.);
    assert_eq!(resources["gemini-5h"]["resets_at"], 11);
    assert_eq!(resources["gemini-weekly"]["remaining_fraction"], 0.);
    assert!(resources["gemini-weekly"].get("resets_at").is_none());
    assert_eq!(
        resources["gemini-argon-wtu-weekly"]["remaining_fraction"],
        0.4
    );
    payload["quota"] = json!({"gemini-argon-wtu-weekly":{"remaining_fraction":0.1}});
    assert_eq!(
        parse(&payload)?.ok_or("missing quota")?.snapshot["resources"]["gemini-argon-wtu-weekly"]["remaining_fraction"],
        0.1
    );
    payload["email"] = json!("");
    assert!(parse(&payload)?.is_none());
    payload["email"] = json!("bad email");
    parse(&payload)
        .err()
        .ok_or("expected invalid reading to fail")?;
    Ok(())
}

#[test]
fn an_amount_only_bucket_keeps_its_amount_without_inventing_a_percent() -> Result<(), Box<dyn Error>>
{
    let payload = json!({
        "product": "antigravity", "email": "a@example.test",
        "quota": {
            "gemini-5h":{"remaining_amount":42,"reset_in_seconds":60},
            "gemini-weekly":{"remaining_amount":7.5,"remaining_fraction":0.25},
            "3p-5h":{"remaining_amount":0}
        }
    });
    let reading =
        antigravity_statusline(&serde_json::to_vec(&payload)?, 100)?.ok_or("missing reading")?;
    let resources = &reading.snapshot["resources"];
    let amount = &resources["gemini-5h"];
    assert_eq!(amount["remaining"], 42.0);
    assert_eq!(amount["unit"], "count");
    assert_eq!(amount["window_seconds"], 18000);
    assert_eq!(amount["resets_at"], 160);
    assert!(amount.get("remaining_fraction").is_none());
    assert_eq!(resources["3p-5h"]["remaining"], 0.0);
    let both = &resources["gemini-weekly"];
    assert_eq!(both["remaining_fraction"], 0.25);
    assert_eq!(both["unit"], "percent");
    assert!(both.get("remaining").is_none());
    Ok(())
}
