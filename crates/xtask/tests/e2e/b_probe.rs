// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_xtask::{
    E2eTarget, EngineExpectation, ProbeError, ProbeMode, ProbeReport, ProbeRequest, Redactor,
    SCHEMA_TEXT, STEP_IDS, schema_step_drift,
};
use serde_json::{Value, json};

use super::scenario::world::{ACCESS, ID_TOKEN, REFRESH};
use super::scenario::{ProbeRaw, Scenario};
use super::support::{blocking_fault, fault};

fn request(only: &[&str], docker: bool, mode: ProbeMode) -> ProbeRequest {
    ProbeRequest {
        target: E2eTarget::t01().expect("T01 target").identity().clone(),
        only: only.iter().map(|id| (*id).to_owned()).collect(),
        docker,
        mode,
        engine: EngineExpectation::Absent,
    }
}

fn a1_redactor() -> Redactor {
    Redactor::new([ACCESS, REFRESH, ID_TOKEN])
}

fn parse(value: &Value, only: &[&str]) -> Result<ProbeReport, ProbeError> {
    ProbeReport::parse(
        value.to_string().as_bytes(),
        &request(only, false, ProbeMode::Sandbox),
        &a1_redactor(),
    )
}

#[tokio::test]
async fn the_real_host_probe_report_validates_and_samples_are_schema_valid() {
    let real = include_bytes!("data/probe_host.json");
    ProbeReport::parse(
        real,
        &request(&STEP_IDS, false, ProbeMode::Host),
        &Redactor::none(),
    )
    .expect("the probe's own --host output validates");

    let scenario = Scenario::new().await;
    for docker in [false, true] {
        let sample = scenario.probe_json(&STEP_IDS, docker);
        let accepted = ProbeReport::parse(
            sample.to_string().as_bytes(),
            &request(&STEP_IDS, docker, ProbeMode::Sandbox),
            &Redactor::none(),
        );
        assert!(
            accepted.is_ok(),
            "sample (docker {docker}) must validate: {accepted:?}"
        );
        for step in STEP_IDS {
            let mut crowded = sample.clone();
            crowded["steps"][step]["surplus"] = json!(1);
            let rejected = ProbeReport::parse(
                crowded.to_string().as_bytes(),
                &request(&STEP_IDS, docker, ProbeMode::Sandbox),
                &Redactor::none(),
            );
            assert!(
                matches!(rejected, Err(ProbeError::Schema(_))),
                "{step} with a surplus key: {rejected:?}"
            );
        }
    }
}

fn schema_error(result: Result<ProbeReport, ProbeError>) -> String {
    match result {
        Err(ProbeError::Schema(message)) => message,
        other => panic!("expected a schema error, got {other:?}"),
    }
}

#[tokio::test]
async fn each_schema_violation_is_rejected_with_its_json_pointer() {
    let scenario = Scenario::new().await;
    let steps = ["J04", "J05", "J06", "J08"];
    let base = scenario.probe_json(&steps, false);
    parse(&base, &steps).expect("the unmutated report is the control");
    type Mutate = fn(&mut Value);
    let table: Vec<(&str, Mutate, &str)> = vec![
        ("schema 1", |v| v["schema"] = json!(1), "/schema"),
        (
            "extra property",
            |v| v["steps"]["J04"]["extra"] = json!(1),
            "/steps/J04",
        ),
        (
            "PASS with a reason",
            |v| v["steps"]["J04"]["reason"] = json!("why"),
            "/steps/J04/reason",
        ),
        (
            "SKIP without a reason",
            |v| v["steps"]["J04"]["result"] = json!("SKIP"),
            "/steps/J04/reason",
        ),
        (
            "missing facts",
            |v| {
                v["steps"]["J04"]
                    .as_object_mut()
                    .expect("step")
                    .remove("facts");
            },
            "/steps/J04",
        ),
        (
            "63 hex digits",
            |v| {
                let digest = v["steps"]["J05"]["facts"]["files"][0]["sha256"]
                    .as_str()
                    .expect("digest")
                    .to_owned();
                v["steps"]["J05"]["facts"]["files"][0]["sha256"] = json!(&digest[1..]);
            },
            "/steps/J05/facts",
        ),
        (
            "hook mode",
            |v| v["steps"]["J06"]["facts"]["hooks"][0]["mode"] = json!("rw-"),
            "/steps/J06/facts",
        ),
        (
            "token kind",
            |v| v["steps"]["J08"]["facts"]["auth_file"]["fields"][0]["kind"] = json!("secret"),
            "/steps/J08/facts",
        ),
        ("no steps", |v| v["steps"] = json!({}), "/steps"),
        (
            "host-only step key",
            |v| v["steps"]["J01"] = v["steps"]["J04"].clone(),
            "/steps",
        ),
        (
            "result in lowercase",
            |v| v["steps"]["J04"]["result"] = json!("pass"),
            "/steps/J04/result",
        ),
        (
            "J05 facts under J04",
            |v| v["steps"]["J04"]["facts"] = v["steps"]["J05"]["facts"].clone(),
            "/steps/J04/facts",
        ),
    ];
    for (name, mutate, pointer) in table {
        let mut broken = base.clone();
        mutate(&mut broken);
        let message = schema_error(parse(&broken, &steps));
        assert!(message.contains(pointer), "{name}: {message}");
    }

    let mut two = base.clone();
    two["schema"] = json!(1);
    two["steps"]["J05"]["facts"]["files"][0]["sha256"] = json!("abc");
    let message = schema_error(parse(&two, &steps));
    assert!(message.contains("/schema"), "{message}");
    assert!(message.contains("/steps/J05/facts"), "{message}");
    assert!(
        message.contains("/steps/J05/facts/files/0/sha256"),
        "the typed second pass names the leaf: {message}"
    );

    let mut leaky = base.clone();
    leaky["steps"]["J04"]["facts"]["user"] = json!(ACCESS);
    leaky["steps"]["J04"]["facts"]["uid"] = json!(REFRESH);
    let message = schema_error(parse(&leaky, &steps));
    assert!(
        !message.contains(ACCESS) && !message.contains(REFRESH),
        "the instance leaks: {message}"
    );
}

#[tokio::test]
async fn stdout_must_be_exactly_one_json_object() {
    let scenario = Scenario::new().await;
    let report = scenario.probe_json(&["J04"], false).to_string();
    let wanted = request(&["J04"], false, ProbeMode::Sandbox);
    let parse = |bytes: &[u8]| ProbeReport::parse(bytes, &wanted, &Redactor::none());
    parse(format!("{report}\n").as_bytes()).unwrap();
    let bom = format!("\u{feff}{report}");
    let two = format!("{report}{report}");
    let garbage = format!("{report} trailing");
    let prose = format!("note: {report}");
    let array = format!("[{report}]");
    let cut = report[..report.len() / 2].to_owned();
    for (name, bytes) in [
        ("empty", String::new()),
        ("prose first", prose),
        ("two objects", two),
        ("truncated", cut),
        ("bom", bom),
        ("array root", array),
        ("trailing garbage", garbage),
    ] {
        assert!(parse(bytes.as_bytes()).is_err(), "{name} must be rejected");
    }
}

#[tokio::test]
async fn report_coverage_must_match_the_request() {
    let scenario = Scenario::new().await;
    let report = scenario.probe_json(&["J11"], false);
    let ok = ProbeReport::parse(
        report.to_string().as_bytes(),
        &request(&["J11"], false, ProbeMode::Sandbox),
        &Redactor::none(),
    );
    ok.unwrap();
    let check = |value: &Value, only: &[&str], docker: bool, mode: ProbeMode| {
        ProbeReport::parse(
            value.to_string().as_bytes(),
            &request(only, docker, mode),
            &Redactor::none(),
        )
    };
    assert!(
        matches!(
            check(&report, &["J11", "J12"], false, ProbeMode::Sandbox),
            Err(ProbeError::Coverage(_))
        ),
        "report lacks J12"
    );
    let wide = scenario.probe_json(&["J11", "J12"], false);
    assert!(
        matches!(
            check(&wide, &["J11"], false, ProbeMode::Sandbox),
            Err(ProbeError::Coverage(_))
        ),
        "extra J12"
    );
    let swapped = scenario.probe_json(&["J12"], false);
    assert!(
        matches!(
            check(&swapped, &["J11"], false, ProbeMode::Sandbox),
            Err(ProbeError::Coverage(_))
        ),
        "J12 for J11, same count"
    );
    assert!(
        matches!(
            check(&report, &["J11"], false, ProbeMode::Host),
            Err(ProbeError::Coverage(_))
        ),
        "mode"
    );
    assert!(
        matches!(
            check(&report, &["J11"], true, ProbeMode::Sandbox),
            Err(ProbeError::Coverage(_))
        ),
        "docker"
    );
}

fn skip_check(scenario: &Scenario, step: &str, check: &str) {
    scenario.patch(step, None, "/checks/0/name", json!(check));
    scenario.patch(step, None, "/checks/0/result", json!("SKIP"));
    scenario.patch(step, None, "/result", json!("SKIP"));
    scenario.patch(step, None, "/reason", json!("skipped by the scenario"));
}

#[tokio::test]
async fn a_failed_probe_check_fails_its_step_with_the_check_name() {
    fault(
        "a check FAIL",
        "J10",
        "probe check fs failed: broken by the scenario",
        |scenario| {
            scenario.patch("J10", None, "/checks/0/name", json!("fs"));
            scenario.patch("J10", None, "/checks/0/result", json!("FAIL"));
            scenario.patch(
                "J10",
                None,
                "/checks/0/detail",
                json!("broken by the scenario"),
            );
            scenario.patch("J10", None, "/result", json!("FAIL"));
            scenario.patch("J10", None, "/reason", json!("broken by the scenario"));
        },
    )
    .await;
}

#[tokio::test]
async fn probe_exit_status_and_stderr_decide_before_the_json_does() {
    let valid = Scenario::new()
        .await
        .probe_json(&["J04"], false)
        .to_string();
    let shared = [
        ("J04", "FAIL"),
        ("J05", "FAIL"),
        ("J06", "FAIL"),
        ("J07", "FAIL"),
        ("J08", "FAIL"),
        ("J09", "FAIL"),
        ("J10", "FAIL"),
        ("J11", "SKIP"),
        ("J12", "SKIP"),
    ];
    for (case, exit, stdout, needle) in [
        (
            "exit 66 with a valid report",
            66,
            valid.as_str(),
            "exited with code 66",
        ),
        ("exit 64", 64, valid.as_str(), "exited with code 64"),
        ("exit 69", 69, valid.as_str(), "exited with code 69"),
        ("exit 70", 70, valid.as_str(), "exited with code 70"),
        (
            "exit 0 with garbage",
            0,
            "not json",
            "not exactly one JSON value",
        ),
    ] {
        let scenario = Scenario::new().await;
        scenario.knobs().probe_raw = Some(ProbeRaw {
            exit,
            stdout: stdout.to_owned(),
            stderr: format!("jq missing near {ACCESS}"),
        });
        let ran = scenario.run().await;
        ran.expect(case, &shared);
        ran.reason_has(case, "J04", needle);
        assert!(
            !ran.stdout.contains(ACCESS) && !ran.stderr.contains(ACCESS),
            "{case}: the stderr tail leaks"
        );
    }
    let mut stuck = Scenario::new().await;
    stuck.budgets.exec = std::time::Duration::from_millis(300);
    stuck.knobs().hang_probe = true;
    let ran = stuck.run().await;
    ran.expect("a probe that never answers", &shared);
    ran.reason_has("timeout", "J04", "timed out");
}

#[test]
fn the_runner_and_the_schema_agree_on_the_step_ids() {
    let schema: Value = serde_json::from_str(SCHEMA_TEXT).expect("schema");
    assert_eq!(schema_step_drift(&schema), None);
    let mut added = schema.clone();
    added["properties"]["steps"]["properties"]["J99"] = json!({});
    assert!(schema_step_drift(&added).is_some_and(|text| text.contains("J99")));
    let mut removed = schema.clone();
    removed["properties"]["steps"]["properties"]
        .as_object_mut()
        .expect("properties")
        .remove("J16");
    assert!(schema_step_drift(&removed).is_some_and(|text| text.contains("J16")));
}

#[tokio::test]
async fn a_probe_skip_in_a_complete_run_is_a_fail_except_the_start_record_case() {
    for state in ["absent", "unreadable", "stale"] {
        let scenario = Scenario::new().await;
        skip_check(&scenario, "J08", "environment");
        scenario.patch("J08", None, "/facts/start_record/state", json!(state));
        let ran = scenario.run().await;
        ran.expect(state, &[("J08", "SKIP")]);
    }
    fault(
        "environment SKIP with a read record",
        "J08",
        "probe skipped environment",
        |scenario| {
            skip_check(scenario, "J08", "environment");
        },
    )
    .await;
    blocking_fault(
        "agent_exec SKIP",
        "J07",
        "probe skipped agent_exec",
        &["J11", "J12"],
        |scenario| {
            skip_check(scenario, "J07", "agent_exec");
        },
    )
    .await;
    fault(
        "engine_denied SKIP",
        "J16",
        "probe skipped engine_denied",
        |scenario| {
            skip_check(scenario, "J16", "engine_denied");
        },
    )
    .await;
    fault(
        "J15 SKIP with the capability on",
        "J15",
        "probe skipped docker_capability",
        |scenario| {
            skip_check(scenario, "J15", "docker_capability");
        },
    )
    .await;
}

#[tokio::test]
async fn target_identity_and_engine_coverage_have_exact_errors() {
    let scenario = Scenario::new().await;
    let base = scenario.probe_json(&["J11"], false);
    for field in [
        "tuple",
        "agent",
        "provider",
        "model",
        "adapter",
        "spec_sha256",
    ] {
        let mut changed = base.clone();
        changed["target"][field] = if field == "spec_sha256" {
            json!("0".repeat(64))
        } else {
            json!("different")
        };
        let error = parse(&changed, &["J11"]).expect_err(field);
        match error {
            ProbeError::Coverage(message) => {
                assert!(message.contains(field), "{field}: {message}");
            }
            other => panic!("{field}: {other:?}"),
        }
    }
    type Mutate = fn(&mut Value);
    let cases: Vec<(&str, Mutate)> = vec![
        ("target absent", |value: &mut Value| {
            value.as_object_mut().expect("report").remove("target");
        }),
        ("agent malformed", |value| {
            value["target"]["agent"] = json!(1);
        }),
        ("digest absent", |value| {
            value["target"]
                .as_object_mut()
                .expect("target")
                .remove("spec_sha256");
        }),
    ];
    for (name, mutate) in cases {
        let mut changed = base.clone();
        mutate(&mut changed);
        assert!(
            matches!(parse(&changed, &["J11"]), Err(ProbeError::Schema(_))),
            "{name}"
        );
    }
    for (report_engine, request_engine) in [
        (None, Some("host.openshell.internal:2376")),
        (Some("host.openshell.internal:2376"), None),
        (
            Some("host.openshell.internal:2376"),
            Some("other.internal:2377"),
        ),
    ] {
        let mut changed = base.clone();
        changed["engine"] = report_engine
            .map(str::to_owned)
            .map_or(Value::Null, Value::String);
        let mut wanted = request(&["J11"], false, ProbeMode::Sandbox);
        wanted.engine = request_engine.map_or(EngineExpectation::Absent, |endpoint| {
            EngineExpectation::Present(endpoint.to_owned())
        });
        let error = ProbeReport::parse(changed.to_string().as_bytes(), &wanted, &Redactor::none())
            .expect_err("engine mismatch");
        assert!(
            matches!(error, ProbeError::Coverage(ref message) if message.contains("engine")),
            "{error}"
        );
    }
}

#[tokio::test]
async fn schema_v3_rejects_old_and_incomplete_auth_and_identity_shapes() {
    let scenario = Scenario::new().await;
    let base = scenario.probe_json(&["J08"], false);
    const AUTH: &str = "/steps/J08/facts/auth_file";
    let mut cases: Vec<(String, Value, &str, Option<&str>)> = Vec::new();
    let mut schema_two = base.clone();
    schema_two["schema"] = json!(2);
    cases.push(("schema 2".to_owned(), schema_two, "/schema", None));
    let mut null_auth = base.clone();
    null_auth["steps"]["J08"]["facts"]["auth_file"] = Value::Null;
    cases.push(("null auth".to_owned(), null_auth, AUTH, None));
    for member in ["state", "path", "mode", "fields"] {
        let mut changed = base.clone();
        changed["steps"]["J08"]["facts"]["auth_file"]
            .as_object_mut()
            .expect("auth")
            .remove(member);
        cases.push((format!("missing {member}"), changed, AUTH, Some(member)));
    }
    for (name, changed, pointer, member) in cases {
        let Err(ProbeError::Schema(message)) = parse(&changed, &["J08"]) else {
            panic!("{name}: not a schema error");
        };
        assert!(message.contains(pointer), "{name}: {message}");
        if let Some(member) = member {
            assert!(
                message.contains(&format!("\"{member}\"")),
                "{name}: {message}"
            );
        }
    }
    assert!(
        parse(&base, &["J08"]).is_ok(),
        "the complete report is accepted"
    );
}

#[tokio::test]
async fn credentialed_exec_and_provider_evidence_cannot_be_skipped_or_absent() {
    for step in ["J07", "J09"] {
        for (name, pointer, value) in [
            ("not attempted", "/facts/agent_exec/attempted", json!(false)),
            (
                "no credential",
                "/facts/agent_exec/credential",
                json!("none"),
            ),
        ] {
            let scenario = Scenario::new().await;
            scenario.patch(step, None, pointer, value);
            let ran = scenario.run().await;
            let blocked: &[(&str, &str)] = if step == "J07" {
                &[("J07", "FAIL"), ("J11", "SKIP"), ("J12", "SKIP")]
            } else {
                &[("J09", "FAIL")]
            };
            ran.expect(&format!("{step} {name}"), blocked);
        }
        let scenario = Scenario::new().await;
        skip_check(&scenario, step, "agent_exec");
        let ran = scenario.run().await;
        let blocked: &[(&str, &str)] = if step == "J07" {
            &[("J07", "FAIL"), ("J11", "SKIP"), ("J12", "SKIP")]
        } else {
            &[("J09", "FAIL")]
        };
        ran.expect(&format!("{step} skipped execution"), blocked);
    }
    fault(
        "provider probe absent",
        "J09",
        "no provider host result",
        |scenario| scenario.patch("J09", None, "/facts/provider_host", Value::Null),
    )
    .await;
}
