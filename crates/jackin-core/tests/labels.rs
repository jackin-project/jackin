// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{
    AccountName, Agent, DockerLabel, ImageId, LabelError, Labels, LaunchId, PlannedLabels,
    RoleName, SandboxLabels, WorkspaceName,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn planned_labels_complete_into_the_documented_label_set() -> TestResult {
    let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let launch = LaunchId::parse("01hzy3k5m7p9r2t4v6w8x0abcd")?;
    let planned = PlannedLabels {
        workspace: WorkspaceName::parse("My.Project")?,
        role: RoleName::parse("backend")?,
        agent: Agent::Codex,
        account: AccountName::parse("work")?,
        launch: launch.clone(),
        docker: DockerLabel::Off,
    };
    let image = ImageId::parse(&format!("sha256:{digest}"))?;
    let labels = planned.complete(&image);
    let set = labels.to_labels();
    let actual: BTreeMap<String, String> = set
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    let expected: BTreeMap<String, String> = [
        ("jackin.dev/managed", "true".to_owned()),
        ("jackin.dev/workspace", "My.Project".to_owned()),
        ("jackin.dev/role", "backend".to_owned()),
        ("jackin.dev/agent", "codex".to_owned()),
        ("jackin.dev/account", "work".to_owned()),
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

#[test]
fn docker_label_holds_off_or_the_engine_port_and_refuses_anything_else() -> TestResult {
    let planned = PlannedLabels {
        workspace: WorkspaceName::parse("proj")?,
        role: RoleName::parse("backend")?,
        agent: Agent::Codex,
        account: AccountName::parse("work")?,
        launch: LaunchId::parse("01hzy3k5m7p9r2t4v6w8x0abcd")?,
        docker: DockerLabel::Engine(std::num::NonZeroU16::new(2376).ok_or("port")?),
    };
    let image = ImageId::parse(&format!("sha256:{}", "ab".repeat(32)))?;
    let labels = planned.complete(&image);
    let set = labels.to_labels();
    let value = |set: &Labels| {
        set.iter()
            .find(|(key, _)| key.to_string() == "jackin.dev/docker")
            .map(|(_, value)| value.to_string())
    };
    assert_eq!(value(&set), Some("2376".to_owned()));
    assert_eq!(SandboxLabels::from_labels(&set)?, labels);
    let rebuilt = |docker: Option<&str>| {
        Labels::from_pairs(
            set.iter()
                .filter(|(key, _)| key.to_string() != "jackin.dev/docker")
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .chain(docker.map(|v| ("jackin.dev/docker".to_owned(), v.to_owned()))),
        )
    };
    assert_eq!(
        SandboxLabels::from_labels(&rebuilt(Some("off"))?)?
            .planned
            .docker,
        DockerLabel::Off
    );
    assert!(matches!(
        SandboxLabels::from_labels(&rebuilt(None)?),
        Err(LabelError::Missing { .. })
    ));
    for bad in ["0", "65536", "abc", "02376", ""] {
        let error = SandboxLabels::from_labels(&rebuilt(Some(bad))?).unwrap_err();
        assert!(
            matches!(error, LabelError::InvalidValue { .. }),
            "{bad:?}: {error:?}"
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
