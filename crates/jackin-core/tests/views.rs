// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use jackin_core::{
    AccountName, Agent, AttemptId, BuildOutput, ClosureViolationView, ImageId, ImageTag,
    InspectView, LaunchId, LsOutput, NameSuffix, Phase, PlannedLabels, PolicyLabel, PolicyStatus,
    ProviderName, RoleName, SandboxLabels, SandboxName, SandboxSlug, SandboxSummary, Sha256,
    SupervisorReading, SupervisorState, Timestamp, WorkspaceName,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;
type TestResult = Fallible<()>;

fn summary(phase: Phase) -> Fallible<SandboxSummary> {
    let image = ImageId::parse(&format!("sha256:{}", "ab".repeat(32)))?;
    let agents = jackin_core::AgentInstances::single(jackin_core::AgentInstance::lead(
        Agent::Codex,
        AccountName::parse("work")?,
    )?)?;
    let planned = PlannedLabels {
        workspace: WorkspaceName::parse("proj")?,
        role: RoleName::parse("sentinel")?,
        agents_hash: agents.hash(),
        lead_agent: Agent::Codex,
        policy: PolicyLabel::of("policy"),
        launch: LaunchId::parse("01hzy3k5m7p9r2t4v6w8x0abcd")?,
    };
    let labels: SandboxLabels = planned.complete(&image, jackin_core::DockerLabel::Off);
    Ok(SandboxSummary {
        name: SandboxName::new(&SandboxSlug::parse("proj")?, &NameSuffix::parse("0abc")?),
        labels,
        phase,
        created_at: Some(Timestamp(1_767_225_600)),
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
        set(&["name", "labels", "phase", "created_at"])
    );
    let unstamped = SandboxSummary {
        created_at: None,
        ..summary(Phase::Ready)?
    };
    let json = round_trip(&unstamped)?;
    assert_eq!(keys(&json)?, set(&["name", "labels", "phase"]));
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

#[test]
fn the_docker_label_shows_as_off_or_an_engine_object_with_the_port_and_the_ca() -> TestResult {
    let ca = jackin_core::CaFingerprint::parse("0123456789abcdef").ok_or("fingerprint")?;
    let engine = jackin_core::DockerLabel::Engine {
        port: std::num::NonZeroU16::new(2376).ok_or("port")?,
        ca,
    };
    let image = ImageId::parse(&format!("sha256:{}", "ab".repeat(32)))?;
    let json = |docker| -> Fallible<serde_json::Value> {
        let mut view = summary(Phase::Ready)?;
        view.labels = view.labels.planned.complete(&image, docker);
        round_trip(&view)
    };
    assert_eq!(
        json(jackin_core::DockerLabel::Off)?["labels"]["docker"],
        "off"
    );
    assert_eq!(
        json(engine)?["labels"]["docker"],
        serde_json::json!({"engine": {"port": 2376, "ca": "0123456789abcdef"}})
    );
    Ok(())
}
