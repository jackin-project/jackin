// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::num::NonZeroU16;

use jackin_core::{Agent, Labels, Phase, SandboxPath};
use jackin_openshell::{InputMode, PowerAction, TermSize};
use jackin_session::{
    AgentMetadataError, AttachError, AttachRequest, IntegrateError, RestartError,
    SandboxObservation, launch_events,
};
use jackin_testkit::Rig;

type TestResult = Result<(), Box<dyn Error>>;

fn metadata_path(handle: &jackin_testkit::SandboxHandle) -> std::path::PathBuf {
    handle.root().join(".jackin/agents.json")
}

fn text(value: &serde_json::Value) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(value)
}

#[tokio::test]
async fn metadata_read_accepts_the_launch_document_and_refuses_untrusted_variants() -> TestResult {
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    let handle = rig.handle(1).await?;
    let path = metadata_path(&handle);
    let bytes = std::fs::read(&path)?;
    let summary = rig
        .world
        .session
        .list(&rig.world.cancel)
        .await?
        .sandboxes
        .into_iter()
        .find(|summary| summary.name == handle.name())
        .ok_or("the launched sandbox is not listed")?;
    let verified = rig
        .world
        .session
        .agent_metadata(&summary, &rig.world.cancel)
        .await?;
    assert_eq!(verified.lead_agent(), Agent::Codex);
    assert_eq!(verified.instances.len(), 1);
    assert_eq!(
        verified.instances.as_slice()[0].agent,
        verified.lead_agent()
    );

    let original: serde_json::Value = serde_json::from_slice(&bytes)?;
    let mut unknown_agent = original.clone();
    unknown_agent[0]["agent"] = serde_json::Value::String("unknown".to_owned());
    let mut unknown_provider = original.clone();
    unknown_provider[0]["provider"] = serde_json::Value::String("unknown".to_owned());
    let mut invalid_account = original.clone();
    invalid_account[0]["account"] = serde_json::Value::String("bad account".to_owned());
    let mut invalid_model = original.clone();
    invalid_model[0]["model"] = serde_json::Value::String("model,forged".to_owned());
    let mut forged_branch = original.clone();
    forged_branch[0]["branch"] = serde_json::Value::String("agent/foreign".to_owned());
    let mut forged_field = original.clone();
    forged_field[0]["extra"] = serde_json::Value::Bool(true);
    let duplicate_agent = serde_json::to_vec(&vec![original[0].clone(), original[0].clone()])?;
    let cases = [
        ("truncated", b"[".to_vec()),
        ("non UTF-8", vec![0xff]),
        ("empty", b"[]".to_vec()),
        ("unknown agent", text(&unknown_agent)?),
        ("unknown provider", text(&unknown_provider)?),
        ("invalid account", text(&invalid_account)?),
        ("invalid model", text(&invalid_model)?),
        ("forged branch", text(&forged_branch)?),
        ("forged field", text(&forged_field)?),
        ("duplicate agent", duplicate_agent),
        (
            "duplicate field",
            br#"[{"agent":"codex","agent":"omp","provider":"codex","account":"work","model":"gpt-5.1-codex","branch":"agent/codex"}]"#.to_vec(),
        ),
        ("oversized", vec![b' '; jackin_session::AGENT_METADATA_LIMIT_BYTES + 1]),
    ];
    for (label, invalid) in cases {
        std::fs::write(&path, invalid)?;
        let result = rig
            .world
            .session
            .agent_metadata(&summary, &rig.world.cancel)
            .await;
        let expected = if label == "oversized" {
            matches!(result, Err(AgentMetadataError::Oversized))
        } else {
            matches!(result, Err(AgentMetadataError::Malformed))
        };
        assert!(expected, "{label}: {result:?}");
    }

    let mut different_set = original.clone();
    different_set[0]["account"] = serde_json::Value::String("other".to_owned());
    std::fs::write(&path, text(&different_set)?)?;
    assert!(matches!(
        rig.world
            .session
            .agent_metadata(&summary, &rig.world.cancel)
            .await,
        Err(AgentMetadataError::HashMismatch)
    ));

    std::fs::write(&path, b"[")?;
    let before_attachments = handle.attachments();
    let pkill = SandboxPath::from_static("/usr/bin/pkill");
    let git = SandboxPath::from_static("/usr/bin/git");
    let before_pkills = handle.runs(&pkill);
    let before_git = handle.runs(&git);
    let size = TermSize {
        cols: NonZeroU16::MIN,
        rows: NonZeroU16::MIN,
    };
    assert!(matches!(
        rig.world
            .session
            .attach(
                AttachRequest {
                    sandbox: handle.name(),
                    size,
                    input: InputMode::Owner,
                    agent: None,
                },
                &rig.world.cancel,
            )
            .await,
        Err(AttachError::AgentMetadata(AgentMetadataError::Malformed))
    ));
    assert!(matches!(
        rig.world
            .session
            .stop_instance(&handle.name(), Agent::Codex, &rig.world.cancel)
            .await,
        Err(IntegrateError::AgentMetadata(AgentMetadataError::Malformed))
    ));
    assert!(matches!(
        rig.world
            .session
            .integrate(&handle.name(), &rig.world.cancel)
            .await,
        Err(IntegrateError::AgentMetadata(AgentMetadataError::Malformed))
    ));
    let (events, feed) = launch_events(jackin_session::LineDelivery::BestEffort);
    drop(feed);
    assert!(matches!(
        rig.world
            .session
            .restart_agent(&handle.name(), events, rig.world.cancel.clone())
            .await,
        Err(RestartError::AgentMetadata(AgentMetadataError::Malformed))
    ));
    assert_eq!(handle.attachments(), before_attachments);
    assert_eq!(handle.runs(&pkill), before_pkills);
    assert_eq!(handle.runs(&git), before_git);
    Ok(())
}

#[tokio::test]
async fn label_hash_and_lead_are_bound_to_a_fresh_sandbox_identity() -> TestResult {
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    let handle = rig.handle(1).await?;
    let summary = rig
        .world
        .session
        .list(&rig.world.cancel)
        .await?
        .sandboxes
        .into_iter()
        .find(|summary| summary.name == handle.name())
        .ok_or("the launched sandbox is not listed")?;
    handle.add_label("jackin.dev/lead-agent", "omp")?;
    let changed_lead = rig
        .world
        .session
        .list(&rig.world.cancel)
        .await?
        .sandboxes
        .into_iter()
        .find(|summary| summary.name == handle.name())
        .ok_or("the changed sandbox is not listed")?;
    assert!(matches!(
        rig.world
            .session
            .agent_metadata(&changed_lead, &rig.world.cancel)
            .await,
        Err(AgentMetadataError::LeadNotMember)
    ));
    assert!(matches!(
        rig.world
            .session
            .agent_metadata(&summary, &rig.world.cancel)
            .await,
        Err(AgentMetadataError::Replaced)
    ));

    handle.add_label("jackin.dev/agents", "00000000000000000000000000000000")?;
    let changed_hash = rig
        .world
        .session
        .list(&rig.world.cancel)
        .await?
        .sandboxes
        .into_iter()
        .find(|summary| summary.name == handle.name())
        .ok_or("the changed sandbox is not listed")?;
    assert!(matches!(
        rig.world
            .session
            .agent_metadata(&changed_hash, &rig.world.cancel)
            .await,
        Err(AgentMetadataError::HashMismatch)
    ));

    handle.forget_created_at();
    assert!(matches!(
        rig.world
            .session
            .agent_metadata_for_sandbox(&handle.name(), &rig.world.cancel)
            .await,
        Err(AgentMetadataError::MissingCreationIdentity)
    ));
    Ok(())
}

#[tokio::test]
async fn missing_lead_keeps_power_safe_and_refuses_agent_actions_without_side_effects() -> TestResult
{
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    let handle = rig.handle(1).await?;
    let labels = Labels::from_pairs(
        handle
            .labels()
            .iter()
            .filter(|(key, _)| key.to_string() != "jackin.dev/lead-agent")
            .map(|(key, value)| (key.to_string(), value.to_string())),
    )?;
    handle.set_labels(labels);
    let sandbox = handle.name();
    let before_attachments = handle.attachments();
    let pkill = SandboxPath::from_static("/usr/bin/pkill");
    let git = SandboxPath::from_static("/usr/bin/git");
    let before_pkills = handle.runs(&pkill);
    let before_git = handle.runs(&git);
    let size = TermSize {
        cols: NonZeroU16::MIN,
        rows: NonZeroU16::MIN,
    };
    assert!(matches!(
        rig.world
            .session
            .attach(
                AttachRequest {
                    sandbox: sandbox.clone(),
                    size,
                    input: InputMode::Owner,
                    agent: None,
                },
                &rig.world.cancel,
            )
            .await,
        Err(AttachError::AgentMetadata(AgentMetadataError::MissingLead))
    ));
    assert!(matches!(
        rig.world
            .session
            .stop_instance(&sandbox, Agent::Codex, &rig.world.cancel)
            .await,
        Err(IntegrateError::AgentMetadata(
            AgentMetadataError::MissingLead
        ))
    ));
    assert!(matches!(
        rig.world
            .session
            .integrate(&sandbox, &rig.world.cancel)
            .await,
        Err(IntegrateError::AgentMetadata(
            AgentMetadataError::MissingLead
        ))
    ));
    let (events, feed) = launch_events(jackin_session::LineDelivery::BestEffort);
    drop(feed);
    assert!(matches!(
        rig.world
            .session
            .restart_agent(&sandbox, events, rig.world.cancel.clone())
            .await,
        Err(RestartError::AgentMetadata(AgentMetadataError::MissingLead))
    ));
    assert_eq!(handle.attachments(), before_attachments);
    assert_eq!(handle.runs(&pkill), before_pkills);
    assert_eq!(handle.runs(&git), before_git);

    let stopped = rig
        .world
        .session
        .power(&sandbox, PowerAction::Stop, &rig.world.cancel)
        .await?;
    let SandboxObservation::Unreadable(stopped) = stopped else {
        return Err("power fabricated readable agent labels".into());
    };
    assert_eq!(stopped.name, sandbox);
    assert_eq!(stopped.phase, Phase::Stopped);
    let started = rig
        .world
        .session
        .power(&sandbox, PowerAction::Start, &rig.world.cancel)
        .await?;
    let SandboxObservation::Unreadable(started) = started else {
        return Err("power fabricated readable agent labels".into());
    };
    assert_eq!(started.name, sandbox);
    assert_eq!(started.phase, Phase::Ready);
    assert_eq!(handle.phase(), Phase::Ready);
    Ok(())
}
