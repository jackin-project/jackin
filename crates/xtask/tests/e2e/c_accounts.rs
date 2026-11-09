// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde_json::{Value, json};

use super::scenario::{Scenario, UsageScript};
use super::support::fault;

fn windows() -> Value {
    json!({"state": "windows", "windows": [
        {"kind": "weekly", "label": "Weekly", "used_percent": 12.5, "resets_at": 1_800_000_000},
        {"kind": "credits", "label": "Credits", "used_percent": null, "remaining": {"kind": "amount", "value": "10", "unit": "credits"}, "resets_at": null},
    ]})
}

fn unavailable(kind: &str) -> Value {
    json!({"state": "unavailable", "reason": {"kind": kind}})
}

fn script(a1: Value, fixtures: Value) -> UsageScript {
    UsageScript {
        a1,
        fixtures,
        drop_account: None,
    }
}

fn usage(scripts: Vec<UsageScript>) -> impl FnOnce(&Scenario) {
    move |scenario: &Scenario| scenario.knobs().usage = Some(scripts)
}

#[tokio::test]
async fn j17_selection_faults() {
    fault(
        "the provider default gives the role account",
        "J17",
        "J17 provider default: the agents label",
        |scenario| {
            scenario.knobs().account_override = Some(("zai".to_owned(), "zai-role".to_owned()));
        },
    )
    .await;
    fault(
        "the role binding gives the provider default",
        "J17",
        "J17 role binding: the agents label",
        |scenario| {
            scenario.knobs().account_override = Some(("zai-role".to_owned(), "zai".to_owned()));
        },
    )
    .await;
    fault(
        "the provider differs from the label",
        "J17",
        "the sandbox providers are",
        |scenario| {
            scenario.knobs().provider_account_override =
                Some(("zai-workspace".to_owned(), "zai-role".to_owned()));
        },
    )
    .await;
    fault(
        "precedence is broken",
        "J17",
        "J17 workspace binding: the agents label",
        |scenario| {
            scenario.knobs().bound_ignored = true;
        },
    )
    .await;
    fault(
        "role resolution falls back to the provider default",
        "J17",
        "J17 role binding: the agents label",
        |scenario| {
            scenario.knobs().account_override =
                Some(("e2e-abcd-fxr".to_owned(), "e2e-abcd-a1".to_owned()));
        },
    )
    .await;
}

#[tokio::test]
async fn j17_usage_content_faults() {
    let expired = unavailable("expired_login");
    fault(
        "a record is missing",
        "J17",
        "no usage record for e2e-abcd-fxr",
        |scenario| {
            let mut missing = script(windows(), expired.clone());
            missing.drop_account = Some("e2e-abcd-fxr".to_owned());
            usage(vec![missing])(scenario);
        },
    )
    .await;
    fault("a window without resets_at", "J17", "has no reset time", usage(vec![script(
        json!({"state": "windows", "windows": [{"kind": "weekly", "label": "w", "used_percent": 1.0}]}),
        expired.clone(),
    )]))
    .await;
    fault("used 101 percent", "J17", "between 0 and 100", usage(vec![script(
        json!({"state": "windows", "windows": [{"kind": "weekly", "label": "w", "used_percent": 101.0, "resets_at": 1}]}),
        expired.clone(),
    )]))
    .await;
    fault(
        "a credits window without a remaining amount",
        "J17",
        "has no remaining amount",
        usage(vec![script(
            json!({"state": "windows", "windows": [
                {"kind": "weekly", "label": "w", "used_percent": 1.0, "resets_at": 1},
                {"kind": "credits", "label": "c", "used_percent": null, "remaining": null, "resets_at": null},
            ]}),
            expired.clone(),
        )]),
    )
    .await;
    fault(
        "a weekly window without a percentage",
        "J17",
        "has no used percentage",
        usage(vec![script(
            json!({"state": "windows", "windows": [
                {"kind": "weekly", "label": "w", "used_percent": null, "remaining": {"kind": "amount", "value": "5", "unit": "x"}, "resets_at": 1},
            ]}),
            expired.clone(),
        )]),
    )
    .await;
    fault(
        "a fixture returns windows",
        "J17",
        "returned windows",
        usage(vec![script(windows(), windows())]),
    )
    .await;
    fault(
        "an untyped fixture reason",
        "J17",
        "untyped reason",
        usage(vec![script(windows(), unavailable("weird"))]),
    )
    .await;
    fault(
        "a credits amount that is not a number",
        "J17",
        "has no remaining amount",
        usage(vec![script(
            json!({"state": "windows", "windows": [{"kind": "weekly", "label": "w", "used_percent": 1.0, "resets_at": 1}, {"kind": "credits", "label": "c", "used_percent": null, "remaining": {"kind": "amount", "value": "lots", "unit": "USD"}, "resets_at": null}]}),
            expired.clone(),
        )]),
    )
    .await;
    fault(
        "a credits amount without a unit",
        "J17",
        "has no remaining amount",
        usage(vec![script(
            json!({"state": "windows", "windows": [{"kind": "weekly", "label": "w", "used_percent": 1.0, "resets_at": 1}, {"kind": "credits", "label": "c", "used_percent": null, "remaining": {"kind": "amount", "value": "5", "unit": ""}, "resets_at": null}]}),
            expired.clone(),
        )]),
    )
    .await;
    fault(
        "an unknown window kind",
        "J17",
        "is unknown",
        usage(vec![script(
            json!({"state": "windows", "windows": [{"kind": "weekly", "label": "w", "used_percent": 1.0, "resets_at": 1}, {"kind": "yearly", "label": "y", "used_percent": 1.0, "resets_at": 1}]}),
            expired.clone(),
        )]),
    )
    .await;
    fault(
        "an other window with neither a percentage nor an amount",
        "J17",
        "has no",
        usage(vec![script(
            json!({"state": "windows", "windows": [{"kind": "weekly", "label": "w", "used_percent": 1.0, "resets_at": 1}, {"kind": "other", "label": "o", "used_percent": null, "remaining": null, "resets_at": null}]}),
            expired.clone(),
        )]),
    )
    .await;
    fault(
        "an other window is not declared for T01",
        "J17",
        "undeclared window",
        |scenario| {
            let mut value = windows();
            value["windows"]
                .as_array_mut()
                .expect("windows")
                .push(json!({"kind": "other", "label": "o", "used_percent": 3.0, "resets_at": 1}));
            usage(vec![script(value, expired.clone())])(scenario);
        },
    )
    .await;
    fault(
        "weekly is required",
        "J17",
        "lacks required weekly",
        usage(vec![script(
            json!({"state": "windows", "windows": [{"kind": "credits", "label": "c", "used_percent": null, "remaining": {"kind": "amount", "value": "5", "unit": "credits"}, "resets_at": null}]}),
            expired.clone(),
        )]),
    )
    .await;
    fault(
        "credits is required",
        "J17",
        "lacks required credits",
        usage(vec![script(
            json!({"state": "windows", "windows": [{"kind": "weekly", "label": "w", "used_percent": 1.0, "resets_at": 1}]}),
            expired.clone(),
        )]),
    )
    .await;
    fault(
        "five-hour is forbidden",
        "J17",
        "undeclared window five_hour",
        |scenario| {
            let mut value = windows();
            value["windows"].as_array_mut().expect("windows").push(
                json!({"kind": "five_hour", "label": "5h", "used_percent": 1.0, "resets_at": 1}),
            );
            usage(vec![script(value, expired.clone())])(scenario);
        },
    )
    .await;
}

#[tokio::test]
async fn j17_usage_rejects_unsupported_and_accepts_complete_windows() {
    let expired = unavailable("expired_login");
    fault(
        "A1 unsupported is fatal",
        "J17",
        "A1 usage is unavailable: unsupported",
        usage(vec![script(unavailable("unsupported"), expired.clone())]),
    )
    .await;
    let complete = Scenario::new().await;
    usage(vec![script(windows(), expired)])(&complete);
    complete
        .run()
        .await
        .expect("weekly and credits without five-hour pass", &[]);
}

#[tokio::test]
async fn j17_transient_usage_failures_are_retried_then_fail() {
    let transient = |kind: &str| script(unavailable(kind), unavailable("expired_login"));
    for kind in ["network", "rate_limited", "provider"] {
        let last_chance = Scenario::new().await;
        usage(vec![
            transient(kind),
            transient(kind),
            transient(kind),
            script(windows(), unavailable("expired_login")),
        ])(&last_chance);
        last_chance
            .run()
            .await
            .expect(&format!("{kind}: success on the 4th attempt"), &[]);
        assert_eq!(last_chance.state().usage_calls, 4, "{kind}: 1 + 3 retries");

        let hopeless = Scenario::new().await;
        usage(vec![transient(kind)])(&hopeless);
        let ran = hopeless.run().await;
        ran.expect(
            &format!("{kind}: failure on every attempt"),
            &[("J17", "FAIL")],
        );
        ran.reason_has(kind, "J17", "after 4 attempts");
        assert_eq!(
            hopeless.state().usage_calls,
            4,
            "{kind}: stops after the 4th attempt"
        );
    }
    for kind in ["expired_login", "no_credentials"] {
        let broken = Scenario::new().await;
        usage(vec![script(
            unavailable(kind),
            unavailable("expired_login"),
        )])(&broken);
        let ran = broken.run().await;
        ran.expect(kind, &[("J17", "FAIL")]);
        assert_eq!(broken.state().usage_calls, 1, "{kind}: no retry");
    }
    let fixture_transient = Scenario::new().await;
    usage(vec![script(windows(), unavailable("network"))])(&fixture_transient);
    let ran = fixture_transient.run().await;
    ran.expect(
        "a fixture transient reason is not expired_login",
        &[("J17", "FAIL")],
    );
    ran.reason_has("fixture reason", "J17", "expected expired_login");
    assert_eq!(
        fixture_transient.state().usage_calls,
        1,
        "a fixture is never retried"
    );
}

#[tokio::test]
async fn j15_skip_rule_follows_the_blocker_file_and_the_engine_flags() {
    let engine = |scenario: &Scenario, on: bool| {
        let mut args = scenario.args.clone();
        if !on {
            args.engine_port = None;
            args.engine_certs = None;
        }
        args
    };
    let mut present_flags = Scenario::new().await;
    present_flags.world.blocker("P0.5-docker.md");
    present_flags.args = engine(&present_flags, true);
    let ran = present_flags.run().await;
    ran.expect("blocker present, engine flags", &[("J15", "SKIP")]);
    assert!(ran.stdout.contains("SKIP=1"));

    let mut present_bare = Scenario::new().await;
    present_bare.world.blocker("P0.5-docker.md");
    present_bare.args = engine(&present_bare, false);
    present_bare.run().await.expect(
        "blocker present, no flags",
        &[("J15", "SKIP"), ("J16", "FAIL")],
    );

    let mut absent_bare = Scenario::new().await;
    absent_bare.args = engine(&absent_bare, false);
    let ran = absent_bare.run().await;
    ran.expect("no blocker, no flags", &[("J15", "FAIL"), ("J16", "FAIL")]);
    ran.reason_has("no flags", "J15", "start the engine");

    let trap = Scenario::new().await;
    trap.world.blocker("P0.5-docker-credentials.md");
    trap.run()
        .await
        .expect("the credentials blocker does not skip J15", &[]);
}
