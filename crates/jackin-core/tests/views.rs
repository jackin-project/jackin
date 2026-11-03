// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use jackin_core::{
    AccountName, Agent, AttemptId, BuildOutput, ClosureViolationView, ImageId, ImageTag,
    InspectView, LaunchId, LsOutput, NameSuffix, Phase, PlannedLabels, PolicyStatus, ProviderName,
    RoleName, SandboxLabels, SandboxName, SandboxSlug, SandboxSummary, Sha256, SupervisorReading,
    SupervisorState, WorkspaceName,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;
type TestResult = Fallible<()>;

fn summary(phase: Phase) -> Fallible<SandboxSummary> {
    let image = ImageId::parse(&format!("sha256:{}", "ab".repeat(32)))?;
    let planned = PlannedLabels {
        workspace: WorkspaceName::parse("proj")?,
        role: RoleName::parse("sentinel")?,
        agent: Agent::Codex,
        account: AccountName::parse("work")?,
        launch: LaunchId::parse("01hzy3k5m7p9r2t4v6w8x0abcd")?,
        docker: jackin_core::DockerLabel::Off,
    };
    let labels: SandboxLabels = planned.complete(&image);
    Ok(SandboxSummary {
        name: SandboxName::new(&SandboxSlug::parse("proj")?, &NameSuffix::parse("0abc")?),
        labels,
        phase,
    })
}

fn keys(value: &serde_json::Value) -> Fallible<BTreeSet<String>> {
    Ok(value.as_object().ok_or("object")?.keys().cloned().collect())
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn round_trip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(
    view: &T,
) -> Fallible<serde_json::Value> {
    let text = serde_json::to_string(view)?;
    assert_eq!(&serde_json::from_str::<T>(&text)?, view);
    Ok(serde_json::from_str(&text)?)
}

#[test]
fn json_views_round_trip_and_keep_the_documented_keys() -> TestResult {
    let ls = LsOutput {
        schema: 1,
        sandboxes: vec![
            summary(Phase::Ready)?,
            summary(Phase::Completed { exit_code: 0 })?,
        ],
    };
    let json = round_trip(&ls)?;
    assert_eq!(keys(&json)?, set(&["schema", "sandboxes"]));
    assert_eq!(
        keys(&json["sandboxes"][0])?,
        set(&["name", "labels", "phase"])
    );
    let readings = [
        SupervisorReading::Unreadable,
        SupervisorReading::Reported(SupervisorState::Exited {
            attempt: AttemptId::FIRST,
            code: 3,
        }),
        SupervisorReading::Invalid("x".to_owned()),
    ];
    let policies = [
        PolicyStatus {
            hash: None,
            version: None,
            loaded: false,
        },
        PolicyStatus {
            hash: Some("abc".to_owned()),
            version: Some(2),
            loaded: true,
        },
    ];
    for supervisor in readings {
        for policy in policies.clone() {
            let view = InspectView {
                schema: 1,
                summary: summary(Phase::Ready)?,
                providers: vec![ProviderName::parse("jackin-work-codex")?],
                policy,
                closure_violations: vec![ClosureViolationView {
                    rule: "r".to_owned(),
                    host: "h".to_owned(),
                    provider_host: "p".to_owned(),
                }],
                supervisor: supervisor.clone(),
            };
            let json = round_trip(&view)?;
            assert_eq!(
                keys(&json)?,
                set(&[
                    "schema",
                    "summary",
                    "providers",
                    "policy",
                    "closure_violations",
                    "supervisor"
                ])
            );
            assert_eq!(keys(&json["policy"])?, set(&["hash", "version", "loaded"]));
            assert_eq!(
                keys(&json["closure_violations"][0])?,
                set(&["rule", "host", "provider_host"])
            );
        }
    }
    let key = Sha256::of(b"key");
    let build = BuildOutput {
        schema: 1,
        image: ImageTag::for_role(&RoleName::parse("sentinel")?, &key),
        id: ImageId::parse(&format!("sha256:{}", "cd".repeat(32)))?,
    };
    let json = round_trip(&build)?;
    assert_eq!(keys(&json)?, set(&["schema", "image", "id"]));
    Ok(())
}
