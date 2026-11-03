// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_xtask::{ExitStatus, LineId, Status, StepId, StepLine, Verdict, render};
use regex::Regex;
use serde_json::json;

use super::scenario::Scenario;
use super::scenario::world::{ACCESS, REFRESH};
use super::support::{IDS, blocking_fault, fault};

fn line(status: Status, id: LineId) -> StepLine {
    let verdict = match status {
        Status::Pass => Verdict::pass("ok"),
        Status::Fail => Verdict::fail("bad"),
        Status::Skip => Verdict::skip("later"),
    };
    StepLine {
        id,
        verdict,
        elapsed: Duration::ZERO,
    }
}

fn step(status: Status) -> StepLine {
    line(status, LineId::Step(StepId::J00))
}

#[test]
fn exit_code_is_zero_only_when_every_line_passes() {
    use Status::{Fail, Pass, Skip};
    let code = |lines: &[StepLine], interrupted| ExitStatus::of(lines, interrupted).code();
    assert_eq!(code(&[step(Pass), step(Pass)], false), 0);
    assert_eq!(code(&[step(Pass), step(Skip)], false), 3);
    assert_eq!(code(&[step(Pass), line(Fail, LineId::Clean)], false), 1);
    assert_eq!(code(&[step(Fail), step(Skip)], false), 1);
    assert_eq!(code(&[], false), 2);
    assert_eq!(code(&[step(Pass)], true), 130);
    assert_eq!(code(&[step(Fail)], true), 130);
    assert_eq!(code(&[line(Fail, LineId::Clean)], true), 130);
}

#[tokio::test]
async fn exit_codes_hold_end_to_end_for_setup_skip_and_interrupts() {
    let setup = Scenario::new().await;
    std::fs::remove_file(setup.world.codex_home.join("auth.json")).expect("remove");
    let ran = setup.run().await;
    assert_eq!(ran.status.code(), 2, "{}", ran.stderr);
    assert!(
        ran.stdout.is_empty(),
        "a setup error prints no step line: {}",
        ran.stdout
    );

    let blocked = Scenario::new().await;
    blocked.world.blocker("P0.5-docker.md");
    let ran = blocked.run().await;
    ran.expect("blocker present with engine flags", &[("J15", "SKIP")]);
    assert!(ran.stdout.contains("SKIP=1"), "{}", ran.stdout);

    let record = Scenario::new().await;
    {
        let (pointer, value) = ("/start_record/state", json!("absent"));
        record.patch("J08", None, &format!("/facts{pointer}"), value);
    }
    record.patch("J08", None, "/checks/0/name", json!("environment"));
    record.patch("J08", None, "/checks/0/result", json!("SKIP"));
    record.patch("J08", None, "/result", json!("SKIP"));
    record.patch("J08", None, "/reason", json!("no start record"));
    let ran = record.run().await;
    ran.expect("start record absent", &[("J08", "SKIP")]);

    let after_fail = Scenario::new().await;
    after_fail.host_rule("role check", Some(1), 1, "control broken");
    after_fail.knobs().cancel_at = Some("ps -eo".to_owned());
    let ran = after_fail.run().await;
    assert_eq!(ran.status.code(), 130, "{}", ran.stdout);
    assert_eq!(ran.status_of("J02"), "FAIL");
    assert_eq!(ran.status_of("J07"), "SKIP");
    assert_eq!(ran.reason_of("J07"), "interrupted");
}

#[tokio::test]
async fn an_interrupt_with_a_failed_cleanup_still_exits_130() {
    let scenario = Scenario::new().await;
    scenario.knobs().cancel_at = Some("ps -eo".to_owned());
    scenario.fake.inject(jackin_testkit::Fault::FailNext {
        op: jackin_openshell::GatewayOp::Delete,
        error: jackin_openshell::GatewayError::Cancelled {
            op: jackin_openshell::GatewayOp::Delete,
        },
    });
    scenario.fake.inject(jackin_testkit::Fault::FailNext {
        op: jackin_openshell::GatewayOp::Delete,
        error: jackin_openshell::GatewayError::Cancelled {
            op: jackin_openshell::GatewayOp::Delete,
        },
    });
    let ran = scenario.run().await;
    assert_eq!(ran.status.code(), 130, "{}", ran.stdout);
    assert_eq!(ran.status_of("CLEAN"), "FAIL", "{}", ran.stdout);
}

#[tokio::test]
async fn every_step_prints_one_line_in_id_order_even_when_they_settle_late() {
    let scenario = Scenario::new().await;
    let ran = scenario.run().await;
    let shape = Regex::new(r"^(J\d\d|CLEAN) (PASS|FAIL|SKIP) +\d+\.\ds .+$").expect("regex");
    let result = Regex::new(r"^RESULT PASS=(\d+) FAIL=(\d+) SKIP=(\d+) elapsed=\d+\.\ds a1_host_login=(unchanged|CHANGED)$")
        .expect("regex");
    let lines: Vec<&str> = ran.stdout.lines().collect();
    let (last, steps) = lines.split_last().expect("lines");
    let ids: Vec<&str> = steps
        .iter()
        .map(|text| {
            assert!(shape.is_match(text), "not a step line: {text}");
            text.split_whitespace().next().expect("id")
        })
        .collect();
    assert_eq!(ids, IDS, "ID order, once each");
    let counts = result
        .captures(last)
        .unwrap_or_else(|| panic!("not a RESULT line: {last}"));
    let total: usize = (1..=3)
        .map(|n| counts[n].parse::<usize>().expect("count"))
        .sum();
    assert_eq!(
        total,
        steps.len(),
        "the counts cover every printed line, CLEAN included"
    );
    assert_eq!(&counts[1], "19");
    let progress: Vec<&str> = ran
        .stderr
        .lines()
        .filter(|text| text.starts_with('J'))
        .map(|text| text.split_whitespace().next().expect("id"))
        .collect();
    let j00 = progress.iter().position(|id| *id == "J00").expect("J00");
    let j03 = progress.iter().position(|id| *id == "J03").expect("J03");
    assert!(j00 > j03, "J00 settles after J03: {progress:?}");

    let broken = Scenario::new().await;
    broken.knobs().drop_steps.push("J04".to_owned());
    let ran = broken.run().await;
    assert_eq!(
        ran.status_of("J04"),
        "FAIL",
        "an error is a FAIL line: {}",
        ran.stdout
    );
    assert_eq!(
        ran.stdout
            .lines()
            .filter(|text| text.starts_with("J04 "))
            .count(),
        1
    );
}

#[test]
fn reasons_are_one_sanitized_line_and_never_empty_for_fail_or_skip() {
    let shown = |verdict: Verdict| {
        render(&StepLine {
            id: LineId::Step(StepId::J05),
            verdict,
            elapsed: Duration::from_secs(1),
        })
    };
    let grammar = Regex::new(r"^\S+ \S+ +\d+\.\ds (.*)$").expect("regex");
    let reason_of = |text: &str| {
        grammar
            .captures(text)
            .map_or(String::new(), |found| found[1].to_owned())
    };
    let long = "x".repeat(5000);
    for (case, input, max) in [
        ("plain", "plain text".to_owned(), 10),
        ("unicode", "caf\u{e9} \u{2713}".to_owned(), 6),
        ("exact", "y".repeat(300), 300),
        ("long", long, 300),
        ("one past", "z".repeat(301), 300),
        ("multibyte boundary", "\u{e9}".repeat(301), 300),
    ] {
        let text = shown(Verdict::fail(input));
        assert_eq!(text.lines().count(), 1, "{case}");
        let reason = reason_of(&text);
        assert_eq!(reason.chars().count(), max, "{case}: characters, not bytes");
        assert!(!reason.contains('\u{fffd}'), "{case}: no split character");
    }
    for hostile in ["a\nb", "a\rb", "a\tb", "a\0b", "a\u{1b}[31mred\u{1b}[0mb"] {
        let text = shown(Verdict::fail(hostile));
        assert_eq!(text.lines().count(), 1, "{hostile:?}");
        assert!(
            !text.contains('\u{1b}') && !text.contains('\0'),
            "{hostile:?}"
        );
    }
    assert!(shown(Verdict::fail("")).ends_with("no reason recorded"));
    assert!(shown(Verdict::skip("")).ends_with("no reason recorded"));
}

#[tokio::test]
async fn a_needle_in_a_reason_is_scrubbed_from_the_line() {
    let scenario = Scenario::new().await;
    scenario.host_rule("role check", Some(1), 1, &format!("denied {ACCESS}"));
    let ran = scenario.run().await;
    assert_eq!(ran.status_of("J02"), "FAIL", "{}", ran.stdout);
    assert!(
        !ran.stdout.contains(ACCESS) && !ran.stderr.contains(ACCESS),
        "{}",
        ran.stdout
    );
    assert!(!ran.stdout.contains(REFRESH));
}

#[tokio::test]
async fn elapsed_time_is_printed_for_each_step_and_the_total() {
    let mut scenario = Scenario::new().await;
    scenario.budgets.global = Duration::from_secs(3600);
    {
        let mut knobs = scenario.knobs();
        knobs
            .advance
            .push(("doctor".to_owned(), Duration::from_secs(1)));
        knobs
            .advance
            .push(("role check".to_owned(), Duration::from_millis(6150)));
        knobs
            .advance
            .push(("build sentinel --json".to_owned(), Duration::from_secs(200)));
    }
    let ran = scenario.run().await;
    ran.expect("injected durations", &[]);
    assert!(ran.stdout.contains("J01 PASS     1.0s"), "{}", ran.stdout);
    assert!(ran.stdout.contains("J02 PASS    12.3s"), "{}", ran.stdout);
    assert!(ran.stdout.contains("200.0s"), "{}", ran.stdout);
    let total = ran.stdout.lines().last().expect("result");
    assert!(
        total.contains("elapsed=2") || total.contains("elapsed=3"),
        "{total}"
    );

    let mut stuck = Scenario::new().await;
    stuck.budgets.step_caps[StepId::J04 as usize] = Duration::from_millis(300);
    stuck.budgets.exec = Duration::from_secs(20);
    stuck.knobs().hang_probe = true;
    let ran = stuck.run().await;
    assert_eq!(ran.status_of("J04"), "FAIL", "{}", ran.stdout);
    assert!(
        ran.stdout.contains("J04 FAIL     0.3s"),
        "the budget, not zero: {}",
        ran.stdout
    );
}

#[tokio::test]
async fn a_failed_prerequisite_blocks_dependents_with_skip_and_exit_one() {
    let blocked: Vec<&str> = [
        "J04", "J05", "J06", "J07", "J08", "J09", "J10", "J11", "J12", "J13", "J14", "J16",
    ]
    .to_vec();
    blocking_fault("J03 fails", "J03", "label", &blocked, |scenario| {
        scenario
            .knobs()
            .drop_labels
            .push("jackin.dev/account".to_owned());
    })
    .await;

    let all: Vec<&str> = IDS[3..18].to_vec();
    let ran = blocking_fault(
        "the first build fails",
        "J00",
        "first build failed",
        &all,
        |scenario| {
            scenario.host_rule("build sentinel --rebuild", None, 1, "no docker");
        },
    )
    .await;
    assert_eq!(ran.status_of("J01"), "PASS");
    assert_eq!(ran.status_of("J02"), "PASS");

    fault(
        "a late J00 mismatch blocks nothing",
        "J00",
        "0.165.0",
        |scenario| {
            scenario.patch(
                "J00",
                None,
                "/facts/codex/version",
                json!("codex-cli 0.165.0"),
            );
        },
    )
    .await;
}
