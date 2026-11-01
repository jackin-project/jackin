// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{
    AccountName, Agent, ImageId, LaunchId, PlannedLabels, RoleName, SandboxLabels, WorkspaceName,
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
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value))
    .collect();
    assert_eq!(actual, expected);
    assert!(set.is_managed());
    assert_eq!(SandboxLabels::from_labels(&set)?, labels);
    Ok(())
}
