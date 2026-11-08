// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use std::num::NonZeroU16;

use jackin_core::{
    AccountName, Agent, CaFingerprint, DockerLabel, ImageId, LabelError, Labels, LaunchId,
    PlannedLabels, PolicyLabel, RoleName, SandboxLabels, WorkspaceName,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn planned_labels_complete_into_the_documented_label_set() -> TestResult {
    let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let launch = LaunchId::parse("01hzy3k5m7p9r2t4v6w8x0abcd")?;
    let agents = jackin_core::AgentInstances::single(jackin_core::AgentInstance::lead(
        Agent::Codex,
        AccountName::parse("work")?,
    )?)?;
    let planned = PlannedLabels {
        workspace: WorkspaceName::parse("My.Project")?,
        role: RoleName::parse("backend")?,
        agents_hash: agents.hash(),
        lead_agent: Agent::Codex,
        policy: PolicyLabel::of("policy"),
        launch: launch.clone(),
    };
    let image = ImageId::parse(&format!("sha256:{digest}"))?;
    let labels = planned.complete(&image, DockerLabel::Off);
    let set = labels.to_labels();
    let actual: BTreeMap<String, String> = set
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    let expected: BTreeMap<String, String> = [
        ("jackin.dev/managed", "true".to_owned()),
        ("jackin.dev/workspace", "My.Project".to_owned()),
        ("jackin.dev/role", "backend".to_owned()),
        (
            "jackin.dev/agents",
            "235f80276e0174fa411c641b8bc2f777".to_owned(),
        ),
        ("jackin.dev/lead-agent", "codex".to_owned()),
        (
            "jackin.dev/policy",
            "823412d1eacb67956220e532959f0104".to_owned(),
        ),
        ("jackin.dev/image", digest[..32].to_owned()),
        ("jackin.dev/launch", launch.to_string()),
        ("jackin.dev/docker", "off".to_owned()),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value))
    .collect();
    assert_eq!(actual, expected);
    assert!(set.is_managed());
    assert_eq!(SandboxLabels::from_labels(&set)?, labels);
    Ok(())
}

fn planned() -> Result<PlannedLabels, Box<dyn std::error::Error>> {
    let agents = jackin_core::AgentInstances::single(jackin_core::AgentInstance::lead(
        Agent::Codex,
        AccountName::parse("work")?,
    )?)?;
    Ok(PlannedLabels {
        workspace: WorkspaceName::parse("proj")?,
        role: RoleName::parse("backend")?,
        agents_hash: agents.hash(),
        lead_agent: Agent::Codex,
        policy: PolicyLabel::of("policy"),
        launch: LaunchId::parse("01hzy3k5m7p9r2t4v6w8x0abcd")?,
    })
}

#[test]
fn lead_agent_label_is_required_and_typed() -> TestResult {
    let image = ImageId::parse(&format!("sha256:{}", "ab".repeat(32)))?;
    let set = planned()?.complete(&image, DockerLabel::Off).to_labels();
    let without_lead = Labels::from_pairs(
        set.iter()
            .filter(|(key, _)| key.to_string() != "jackin.dev/lead-agent")
            .map(|(key, value)| (key.to_string(), value.to_string())),
    )?;
    assert!(matches!(
        SandboxLabels::from_labels(&without_lead),
        Err(LabelError::Missing {
            key: "jackin.dev/lead-agent"
        })
    ));
    let invalid_lead = Labels::from_pairs(
        set.iter()
            .filter(|(key, _)| key.to_string() != "jackin.dev/lead-agent")
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .chain([("jackin.dev/lead-agent".to_owned(), "unknown".to_owned())]),
    )?;
    assert!(matches!(
        SandboxLabels::from_labels(&invalid_lead),
        Err(LabelError::Invalid {
            key: "jackin.dev/lead-agent",
            ..
        })
    ));
    Ok(())
}

fn docker_value(set: &Labels) -> Option<String> {
    set.iter()
        .find(|(key, _)| key.to_string() == "jackin.dev/docker")
        .map(|(_, value)| value.to_string())
}

fn with_docker(set: &Labels, docker: Option<&str>) -> Result<Labels, LabelError> {
    Labels::from_pairs(
        set.iter()
            .filter(|(key, _)| key.to_string() != "jackin.dev/docker")
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .chain(docker.map(|v| ("jackin.dev/docker".to_owned(), v.to_owned()))),
    )
}

#[test]
fn every_docker_label_value_passes_the_label_grammar_and_round_trips() -> TestResult {
    let image = ImageId::parse(&format!("sha256:{}", "ab".repeat(32)))?;
    let fingerprints = ["0123456789abcdef", "ffffffffffffffff", "0000000000000000"];
    let mut variants = vec![(DockerLabel::Off, "off".to_owned())];
    for port in [1_u16, 2376, 65535] {
        for text in fingerprints {
            let ca = CaFingerprint::parse(text).ok_or("fingerprint")?;
            variants.push((
                DockerLabel::Engine {
                    port: NonZeroU16::new(port).ok_or("port")?,
                    ca,
                },
                format!("engine-{port}-{text}"),
            ));
        }
    }
    for (docker, text) in variants {
        let labels = planned()?.complete(&image, docker);
        let set = labels.to_labels();
        let pairs = set
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()));
        let checked = Labels::from_pairs(pairs)?;
        assert_eq!(checked, set, "{text}");
        assert_eq!(docker_value(&set), Some(text.clone()));
        assert_eq!(SandboxLabels::from_labels(&checked)?, labels, "{text}");
    }
    Ok(())
}

#[test]
fn docker_label_refuses_anything_but_off_or_a_canonical_port_and_a_lowercase_fingerprint()
-> TestResult {
    let image = ImageId::parse(&format!("sha256:{}", "ab".repeat(32)))?;
    let set = planned()?.complete(&image, DockerLabel::Off).to_labels();
    assert!(matches!(
        SandboxLabels::from_labels(&with_docker(&set, None)?),
        Err(LabelError::Missing { .. })
    ));
    let hex = "0123456789abcdef";
    let bad = [
        String::new(),
        "2376".to_owned(),
        "engine".to_owned(),
        "engine-2376".to_owned(),
        "engine-2376-".to_owned(),
        format!("engine-2376-{}", &hex[..15]),
        format!("engine-2376-{hex}0"),
        format!("engine-2376-{}", hex.to_uppercase()),
        format!("engine-2376-{}g", &hex[..15]),
        format!("engine-02376-{hex}"),
        format!("engine-0-{hex}"),
        format!("engine-65536-{hex}"),
        format!("engine--{hex}"),
        format!("engine-+2376-{hex}"),
        format!("Engine-2376-{hex}"),
        format!("engine:2376:{hex}"),
        format!("engine-2376-{hex}-x"),
    ];
    for value in bad {
        let error = match with_docker(&set, Some(&value)) {
            Err(error) => error,
            Ok(labels) => SandboxLabels::from_labels(&labels).unwrap_err(),
        };
        assert!(
            matches!(error, LabelError::InvalidValue { .. }),
            "{value:?}: {error:?}"
        );
    }
    Ok(())
}

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn from_pairs_keeps_foreign_labels_that_the_gateway_grammar_allows_and_rejects_the_rest()
-> TestResult {
    let long = "a".repeat(63);
    let accepted = pairs(&[
        ("team", "runtime"),
        ("k8s.example.com/Tier-1_x.y", ""),
        ("plain", &long),
    ]);
    let labels = Labels::from_pairs(accepted.clone())?;
    let mut kept: Vec<(String, String)> = labels
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    kept.sort();
    let mut expected = accepted;
    expected.sort();
    assert_eq!(kept, expected);
    assert!(!labels.is_managed());

    let too_long = "a".repeat(64);
    let bad_keys = [
        "",
        "-lead",
        "trail-",
        "has space",
        "UPPER.example.com/x",
        "example..com/x",
        "-example.com/x",
        "/x",
        &too_long,
    ];
    for key in bad_keys {
        let error = Labels::from_pairs(pairs(&[(key, "v")])).unwrap_err();
        assert!(
            matches!(error, LabelError::InvalidKey { .. }),
            "{key}: {error:?}"
        );
    }
    for value in [&too_long[..], "-lead", "trail-", "has space"] {
        let error = Labels::from_pairs(pairs(&[("ok", value)])).unwrap_err();
        assert!(
            matches!(error, LabelError::InvalidValue { .. }),
            "{value}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn differences_compare_the_whole_label_set_in_both_directions() -> TestResult {
    let image = ImageId::parse(&format!("sha256:{}", "ab".repeat(32)))?;
    let wanted = planned()?.complete(&image, DockerLabel::Off);
    let set = wanted.to_labels();
    assert!(set.differences(&wanted).is_empty());

    let pairs = || {
        set.iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
    };
    let extended =
        Labels::from_pairs(pairs().chain([("example.com/owner".to_owned(), "x".to_owned())]))?;
    let shown: Vec<String> = extended
        .differences(&wanted)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        shown,
        ["example.com/owner is x on the sandbox and (none) in this launch"]
    );

    let reduced = Labels::from_pairs(pairs().filter(|(key, _)| key != "jackin.dev/managed"))?;
    let shown: Vec<String> = reduced
        .differences(&wanted)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        shown,
        ["jackin.dev/managed is (none) on the sandbox and true in this launch"]
    );
    Ok(())
}
