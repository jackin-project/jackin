// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_openshell::Gateway as _;
use serde_json::{Value, json};

use super::scenario::{ExportMode, J05Fault, Scenario};
use super::support::{blocking_fault, fault};

const BLOCKED_BY_J03: [&str; 12] = [
    "J04", "J05", "J06", "J07", "J08", "J09", "J10", "J11", "J12", "J13", "J14", "J16",
];

fn canonical_role(scenario: &Scenario) -> String {
    let path = std::fs::canonicalize(&scenario.world.role).expect("role");
    format!("role check {}", path.display())
}

#[tokio::test]
async fn a_consistent_scripted_journey_prints_all_pass_and_exits_zero() {
    let mut scenario = Scenario::new().await;
    for (round, tag) in ["abcd", "beef"].into_iter().enumerate() {
        scenario.tag = tag.to_owned();
        let ran = scenario.run().await;
        ran.expect(&format!("round {round}"), &[]);
        let sandboxes = scenario
            .gateway
            .find_sandboxes(&jackin_openshell::SandboxQuery::Labels(
                jackin_core::LabelSelector::managed(),
            ))
            .await
            .expect("find");
        let names: Vec<String> = sandboxes.iter().map(|view| view.name.to_string()).collect();
        assert_eq!(
            names,
            ["jackin-mine-0000"],
            "only the foreign sandbox remains"
        );
        let providers: Vec<String> = scenario
            .fake
            .providers()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            providers,
            ["jackin-mine-codex"],
            "only the foreign provider remains"
        );
    }
}

#[tokio::test]
async fn j00_image_build_faults() {
    let after_build: Vec<&str> = (3..18)
        .map(|n| {
            [
                "J00", "J01", "J02", "J03", "J04", "J05", "J06", "J07", "J08", "J09", "J10", "J11",
                "J12", "J13", "J14", "J15", "J16", "J17",
            ][n]
        })
        .collect();
    let ran = blocking_fault(
        "the first build exits 1",
        "J00",
        "the first build failed",
        &after_build,
        |scenario| {
            scenario.host_rule("build sentinel --rebuild", None, 1, "no builder");
        },
    )
    .await;
    assert_eq!(ran.status_of("J01"), "PASS");
    fault(
        "second image id differs",
        "J00",
        "differs from the first",
        |scenario| {
            scenario.knobs().second_build_id = Some(format!("sha256:{}", "ab".repeat(32)));
        },
    )
    .await;
    fault(
        "no installing line",
        "J00",
        "installing jackin-sentinel-probe",
        |scenario| {
            scenario.knobs().first_build_without_install = true;
        },
    )
    .await;
    fault(
        "second build says built",
        "J00",
        "did not report every stage cached",
        |scenario| {
            scenario.knobs().second_build_stage = Some("image derived: built".to_owned());
        },
    )
    .await;
    fault("version 0.165.0 (late)", "J00", "0.165.0", |scenario| {
        scenario.patch(
            "J00",
            None,
            "/facts/agent/version",
            json!("codex-cli 0.165.0"),
        );
    })
    .await;
    for (case, pointer, value) in [
        (
            "agent path is another file (late)",
            "/facts/agent/path",
            json!("/usr/bin/codex"),
        ),
        (
            "agent binary is absent (late)",
            "/facts/agent/present",
            json!(false),
        ),
        (
            "agent binary is not executable (late)",
            "/facts/agent/executable",
            json!(false),
        ),
    ] {
        fault(
            case,
            "J00",
            "/usr/local/bin/codex is not an executable file in the image",
            |scenario| scenario.patch("J00", None, pointer, value),
        )
        .await;
    }
    fault(
        "agent version is missing (late)",
        "J00",
        "no version",
        |scenario| scenario.patch("J00", None, "/facts/agent/version", Value::Null),
    )
    .await;
    fault(
        "probe digest differs (late)",
        "J00",
        "installed probe digest differs",
        |scenario| {
            scenario.exec_rule(
                "sha256sum /usr/local/bin/jackin-sentinel-probe",
                0,
                "deadbeef  /usr/local/bin/jackin-sentinel-probe\n",
            );
        },
    )
    .await;
}

#[tokio::test]
async fn j01_doctor_faults() {
    let lines = |warn: bool, drop_health: bool| -> Vec<String> {
        let names = [
            "openshell-binary",
            "ssh-binary",
            "git-binary",
            "gateway-registered",
            "gateway-health",
            "pinned-version",
            "engine-socket",
        ];
        names
            .iter()
            .filter(|name| !(drop_health && **name == "gateway-health"))
            .map(|name| {
                let status = if warn && *name == "gateway-health" {
                    "warn"
                } else {
                    "pass"
                };
                format!("{status} {name:<18} 0.1.7")
            })
            .collect()
    };
    fault("a warn line", "J01", "gateway-health is warn", |scenario| {
        scenario.knobs().doctor = Some(lines(true, false));
    })
    .await;
    fault(
        "a missing line",
        "J01",
        "printed no gateway-health",
        |scenario| scenario.knobs().doctor = Some(lines(false, true)),
    )
    .await;
    fault("health version 0.1.5", "J01", "runs 0.1.5", |scenario| {
        scenario.fake.set_version("0.1.5");
    })
    .await;
    fault("not serving", "J01", "not serving", |scenario| {
        scenario.knobs().unserving_at_doctor = true;
    })
    .await;
}

#[tokio::test]
async fn j02_strict_manifest_faults() {
    fault("control fails", "J02", "control role failed", |scenario| {
        let needle = canonical_role(scenario);
        scenario.host_rule(&needle, Some(1), 1, "broken");
    })
    .await;
    fault(
        "mutated exits 0",
        "J02",
        "passed `role check`",
        |scenario| scenario.host_rule("role-copies/strict", None, 0, ""),
    )
    .await;
    fault(
        "mutated does not name the field",
        "J02",
        "did not name mystery_field",
        |scenario| {
            scenario.host_rule("role-copies/strict", None, 1, "something is wrong");
        },
    )
    .await;
}

#[tokio::test]
async fn j03_one_sandbox_per_launch_faults() {
    type Tweak = Box<dyn Fn(&Scenario)>;
    let image = |value: String| -> Tweak {
        Box::new(move |s: &Scenario| {
            s.knobs().set_labels = vec![("jackin.dev/image".to_owned(), value.clone())];
        })
    };
    let table: Vec<(&str, &str, Tweak)> = vec![
        (
            "a retry yields two sandboxes",
            "sandboxes carry launch ID",
            Box::new(|s| s.knobs().duplicate_on_retry = true),
        ),
        (
            "the retry renames",
            "the retry changed the sandbox name",
            Box::new(|s| s.knobs().rename_on_retry = true),
        ),
        (
            "the docker label is missing",
            "label jackin.dev/docker is missing",
            Box::new(|s| s.knobs().drop_labels.push("jackin.dev/docker".to_owned())),
        ),
        (
            "the docker label is on for a sandbox without the capability",
            "label jackin.dev/docker is",
            Box::new(|s| {
                s.knobs().set_labels = vec![(
                    "jackin.dev/docker".to_owned(),
                    "engine-2376-0123456789abcdef".to_owned(),
                )];
            }),
        ),
        (
            "a label is missing",
            "label jackin.dev/agents is missing",
            Box::new(|s| s.knobs().drop_labels.push("jackin.dev/agents".to_owned())),
        ),
        (
            "image label of 31 characters",
            "jackin.dev/image has 31",
            image("a".repeat(31)),
        ),
        (
            "image label of 33 characters",
            "jackin.dev/image has 33",
            image("a".repeat(33)),
        ),
        ("uppercase hex", "non-lowercase-hex", image("A".repeat(32))),
        (
            "a name without the suffix",
            "is not jackin-<slug>-<4>",
            Box::new(|s| s.knobs().name_shape = Some("jackin-main".to_owned())),
        ),
    ];
    for (case, needle, tweak) in table {
        blocking_fault(case, "J03", needle, &BLOCKED_BY_J03, |scenario| {
            tweak(scenario);
        })
        .await;
    }
    let control = Scenario::new().await;
    control.knobs().name_shape = Some("jackin-sevenxx-ab12".to_owned());
    assert_eq!("jackin-sevenxx-ab12".len(), 19);
    control
        .run()
        .await
        .expect("a 19-character name is the pass control", &[]);
}

#[tokio::test]
async fn j04_identity_faults() {
    fault(
        "the probe says sudo works",
        "J04",
        "the probe says sudo works",
        |scenario| {
            scenario.patch("J04", None, "/facts/sudo/works", json!(true));
        },
    )
    .await;
    fault(
        "docker binary disagreement",
        "J04",
        "exec says true",
        |scenario| {
            scenario.exec_rule(
                "echo uid=",
                0,
                "uid=1000\nuser=sandbox\ndocker=yes\nsudo=no\n",
            );
        },
    )
    .await;
    fault(
        "uid 1001",
        "J04",
        "expected sandbox/1000/1000",
        |scenario| {
            scenario.patch("J04", None, "/facts/uid", json!(1001));
        },
    )
    .await;
    fault(
        "both sides agree on the wrong value",
        "J04",
        "sudo works in the sandbox",
        |scenario| {
            scenario.patch("J04", None, "/facts/sudo/works", json!(true));
            scenario.exec_rule(
                "echo uid=",
                0,
                "uid=1000\nuser=sandbox\ndocker=no\nsudo=yes\n",
            );
        },
    )
    .await;
}

#[tokio::test]
async fn j05_upload_faults() {
    for (case, needle, which) in [
        ("a file is missing", "the probe lacks", J05Fault::DropFile),
        ("an extra file", "the probe has extra", J05Fault::ExtraFile),
        (
            "one digest differs",
            "the digest differs for",
            J05Fault::BadDigest,
        ),
        (
            "the ignored file is included",
            "the ignored file reached the sandbox",
            J05Fault::IncludeIgnored,
        ),
        ("a newline in a path", r"a\nb.txt", J05Fault::NewlineName),
    ] {
        fault(case, "J05", needle, |scenario| {
            scenario.knobs().j05_fault = Some(which);
        })
        .await;
    }
    fault(
        "the host status changes before J13 ends",
        "J05",
        "changed between J03 and J13",
        |scenario| {
            scenario.knobs().export_mode = ExportMode::StageFile;
        },
    )
    .await;
}
