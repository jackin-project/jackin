// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU16;
use std::time::Duration;

use jackin_core::{
    Account, AccountName, Agent, AgentInstance, AgentInstances, ConfigEdit, CredentialSource,
    EnvName, FileMode, HostPath, Provider, SandboxPath, Sha256,
};
use jackin_openshell::{GatewayError, TermSize};
use jackin_session::{
    AttachError, AttachRequest, IntegrateError, LaunchError, LaunchRequest, plan_launch,
};
use jackin_testkit::{Outcome, ProgramScript, Rig, SandboxHandle};
use tokio_util::sync::CancellationToken;

use super::support::{
    LiveInstance, TestResult, failure, guard, identity_queries, launch_rig, multi_agent_rig, ready,
    stale_pid,
};

fn members(lead: Agent) -> Result<AgentInstances, Box<dyn std::error::Error>> {
    let codex = AgentInstance::new(
        Agent::Codex,
        Provider::Zai,
        AccountName::parse("work")?,
        "glm-5.3".to_owned(),
    );
    let claude = AgentInstance::new(
        Agent::ClaudeCode,
        Provider::Zai,
        AccountName::parse("zai-work")?,
        "glm-5.3".to_owned(),
    );
    Ok(AgentInstances::new(if lead == Agent::ClaudeCode {
        vec![claude, codex]
    } else {
        vec![codex, claude]
    })?)
}

async fn configured() -> Result<Rig, Box<dyn std::error::Error>> {
    let rig = multi_agent_rig().await?;
    let key = HostPath::parse(&rig.home.as_path().join("zai.key"))?;
    rig.world
        .fs
        .write_new(&key, b"SYNTHETIC-ZAI-IDENTITY", FileMode(0o600))
        .await?;
    let account = Account::new(
        AccountName::parse("zai-work")?,
        Provider::Zai,
        CredentialSource::File { path: key },
    )?;
    rig.world
        .config
        .edit(vec![ConfigEdit::AddAccount(account)])
        .await?;
    let config = rig.world.config.load().await?;
    let codex = config
        .account(&AccountName::parse("work")?)
        .ok_or("Codex account")?;
    rig.world
        .session
        .sync_account(codex, Agent::Codex, &CancellationToken::new())
        .await?;
    Ok(rig)
}

async fn run(rig: &Rig, request: LaunchRequest) -> Result<Outcome, Box<dyn std::error::Error>> {
    let config = rig.world.config.load().await?;
    let plan = plan_launch(&config, &rig.loaded, request)?;
    Ok(rig
        .run_planned(&rig.world.session, plan, CancellationToken::new())
        .await)
}

fn size() -> Result<TermSize, Box<dyn std::error::Error>> {
    Ok(TermSize {
        cols: NonZeroU16::new(100).ok_or("columns")?,
        rows: NonZeroU16::new(30).ok_or("rows")?,
    })
}

async fn wait_main(handle: &SandboxHandle, expected: &[u8]) -> TestResult {
    for _ in 0..400 {
        if handle.main_input() == expected {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(handle.main_input(), expected);
    Ok(())
}

#[tokio::test]
async fn nonlexical_lead_attach_routes_main_and_secondary_terminal_contexts() -> TestResult {
    guard(async {
        let rig = configured().await?;
        let request = rig.request(31)?.instances(members(Agent::ClaudeCode)?);
        let target = ready(run(&rig, request).await?)?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
        let mut owner = AttachRequest::owner(target.clone(), size()?);
        owner.agent = Some(Agent::ClaudeCode);
        let mut lead = rig
            .world
            .session
            .attach(owner, &CancellationToken::new())
            .await?;
        assert!(handle.shells().is_empty());
        lead.input(b"lead-identity-input".to_vec()).await?;
        wait_main(&handle, b"lead-identity-input").await?;
        rig.gateway().script_program(
            &SandboxPath::from_static("/usr/bin/pkill"),
            ProgramScript {
                exit_code: 1,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
        );
        let mut other = AttachRequest::owner(target.clone(), size()?);
        other.agent = Some(Agent::Codex);
        let mut secondary = rig
            .world
            .session
            .attach(other, &CancellationToken::new())
            .await?;
        let shells = handle.shells();
        assert_eq!(shells.len(), 1);
        assert_eq!(shells[0].workdir.as_str(), "/sandbox/work/codex/project");
        assert_eq!(
            shells[0]
                .env
                .get(&EnvName::parse("HOME")?)
                .map(String::as_str),
            Some("/sandbox/.jackin/homes/codex")
        );
        secondary
            .input(b"printf secondary-input\n".to_vec())
            .await?;
        assert_eq!(handle.main_input(), b"lead-identity-input");
        secondary.detach().await?;
        lead.detach().await?;
        let mut absent = AttachRequest::owner(target, size()?);
        absent.agent = Some(Agent::GrokBuild);
        assert!(matches!(
            rig.world
                .session
                .attach(absent, &CancellationToken::new())
                .await,
            Err(AttachError::NotFound(_))
        ));
        assert_eq!(handle.shells().len(), 1);
        assert_eq!(handle.attachments(), 0);
        Ok(())
    })
    .await
}

async fn align_non_agents_gateway_labels(
    rig: &Rig,
    handle: &SandboxHandle,
    request: LaunchRequest,
) -> TestResult {
    let before = (
        handle.files(),
        handle.labels(),
        handle.supervisor(),
        rig.gateway().providers(),
        rig.gateway().sandboxes().len(),
        handle.main_input(),
        handle.shells(),
        handle.attachments(),
    );
    let probe = failure(run(rig, request).await?)?;
    assert_eq!(
        before,
        (
            handle.files(),
            handle.labels(),
            handle.supervisor(),
            rig.gateway().providers(),
            rig.gateway().sandboxes().len(),
            handle.main_input(),
            handle.shells(),
            handle.attachments(),
        )
    );
    let LaunchError::StaleSandbox { differences, .. } = *probe.error else {
        return Err("fixture probe did not return typed stale differences".into());
    };
    for difference in differences {
        if difference.key.to_string() != jackin_core::AGENTS_LABEL {
            handle.add_label(&difference.key.to_string(), &difference.wanted.to_string())?;
        }
    }
    Ok(())
}

fn agents_label_refusal(outcome: Outcome, found: &str, wanted: &str) -> TestResult {
    let failure = failure(outcome)?;
    let LaunchError::StaleSandbox { differences, .. } = *failure.error else {
        return Err("expected an agents-label stale refusal".into());
    };
    assert!(
        differences
            .iter()
            .all(|difference| difference.key.to_string() == jackin_core::AGENTS_LABEL),
        "{differences:?}"
    );
    assert!(
        differences.iter().any(|difference| {
            difference.key.to_string() == jackin_core::AGENTS_LABEL
                && difference.found.to_string() == found
                && difference.wanted.to_string() == wanted
                && found != wanted
        }),
        "expected the actual agents-label hash mismatch: {differences:?}"
    );
    Ok(())
}

#[tokio::test]
async fn retry_keeps_the_current_lead_but_refuses_a_lead_only_change_without_mutation() -> TestResult
{
    guard(async {
        let rig = configured().await?;
        let request = rig.request(32)?.instances(members(Agent::ClaudeCode)?);
        let target = ready(run(&rig, request.clone()).await?)?;
        let same = ready(run(&rig, request).await?)?;
        assert_eq!(same.sandbox, target.sandbox);
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
        let changed = rig.request(32)?.instances(members(Agent::Codex)?);
        align_non_agents_gateway_labels(&rig, &handle, changed.clone()).await?;
        let before = (
            handle.files(),
            handle.labels(),
            handle.supervisor(),
            rig.gateway().providers(),
            rig.gateway().sandboxes().len(),
            handle.main_input(),
            handle.shells(),
            handle.attachments(),
        );
        agents_label_refusal(
            run(&rig, changed).await?,
            &members(Agent::ClaudeCode)?.agents_label(),
            &members(Agent::Codex)?.agents_label(),
        )?;
        assert_eq!(
            before,
            (
                handle.files(),
                handle.labels(),
                handle.supervisor(),
                rig.gateway().providers(),
                rig.gateway().sandboxes().len(),
                handle.main_input(),
                handle.shells(),
                handle.attachments(),
            )
        );
        Ok(())
    })
    .await
}

async fn legacy(rig: &Rig, id: u8) -> Result<SandboxHandle, Box<dyn std::error::Error>> {
    let target = ready(run(rig, rig.request(id)?.instances(members(Agent::ClaudeCode)?)).await?)?;
    let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
    let mut old = members(Agent::ClaudeCode)?.as_slice().to_vec();
    old.sort_by_key(AgentInstance::item);
    let hash = Sha256::of(
        old.iter()
            .map(AgentInstance::item)
            .collect::<Vec<_>>()
            .join(",")
            .as_bytes(),
    );
    handle.add_label(jackin_core::AGENTS_LABEL, &hash.as_str()[..32])?;
    std::fs::write(
        handle.root().join(".jackin/agents.json"),
        serde_json::to_vec(&old)?,
    )?;
    Ok(handle)
}

#[tokio::test]
async fn old_sorted_identity_that_matches_new_lead_order_is_refused_on_retry() -> TestResult {
    guard(async {
        let rig = configured().await?;
        let handle = legacy(&rig, 33).await?;
        assert_eq!(
            handle.file(&SandboxPath::from_static("/sandbox/.jackin/agents.json")),
            Some(serde_json::to_vec(members(Agent::ClaudeCode)?.as_slice())?)
        );
        let changed = rig.request(33)?.instances(members(Agent::Codex)?);
        align_non_agents_gateway_labels(&rig, &handle, changed.clone()).await?;
        let old_hash = handle
            .labels()
            .iter()
            .find(|(key, _)| key.to_string() == jackin_core::AGENTS_LABEL)
            .map(|(_, value)| value.to_string())
            .ok_or("agents label")?;
        let before = (
            handle.files(),
            handle.labels(),
            handle.supervisor(),
            rig.gateway().providers(),
            rig.gateway().sandboxes().len(),
            handle.main_input(),
            handle.shells(),
            handle.attachments(),
        );
        agents_label_refusal(
            run(&rig, changed).await?,
            &old_hash,
            &members(Agent::Codex)?.agents_label(),
        )?;
        assert_eq!(
            before,
            (
                handle.files(),
                handle.labels(),
                handle.supervisor(),
                rig.gateway().providers(),
                rig.gateway().sandboxes().len(),
                handle.main_input(),
                handle.shells(),
                handle.attachments(),
            )
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn implicit_attach_refuses_ambiguous_old_identity_before_opening_a_terminal() -> TestResult {
    guard(async {
        let rig = configured().await?;
        let handle = legacy(&rig, 34).await?;
        let before = (
            handle.files(),
            handle.labels(),
            handle.supervisor(),
            rig.gateway().providers(),
        );
        let request = AttachRequest::viewer(handle.name(), size()?);
        assert!(matches!(
            rig.world
                .session
                .attach(request, &CancellationToken::new())
                .await,
            Err(AttachError::Gateway(GatewayError::Protocol { .. }))
        ));
        assert_eq!(
            before,
            (
                handle.files(),
                handle.labels(),
                handle.supervisor(),
                rig.gateway().providers()
            )
        );
        assert_eq!(handle.attachments(), 0);
        assert!(handle.shells().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn explicit_attach_refuses_ambiguous_old_identity_before_opening_a_terminal() -> TestResult {
    guard(async {
        let rig = configured().await?;
        let handle = legacy(&rig, 35).await?;
        let before = (
            handle.files(),
            handle.labels(),
            handle.supervisor(),
            rig.gateway().providers(),
        );
        for agent in [Agent::Codex, Agent::ClaudeCode] {
            let mut request = AttachRequest::viewer(handle.name(), size()?);
            request.agent = Some(agent);
            assert!(matches!(
                rig.world
                    .session
                    .attach(request, &CancellationToken::new())
                    .await,
                Err(AttachError::Gateway(GatewayError::Protocol { .. }))
            ));
        }
        assert_eq!(
            before,
            (
                handle.files(),
                handle.labels(),
                handle.supervisor(),
                rig.gateway().providers()
            )
        );
        assert_eq!(handle.attachments(), 0);
        assert!(handle.shells().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn integrate_refuses_ambiguous_old_identity_without_merging_a_branch() -> TestResult {
    guard(async {
        let rig = configured().await?;
        let handle = legacy(&rig, 36).await?;
        let head = handle.git(&["rev-parse", "HEAD"]).await?;
        let before = (
            handle.files(),
            handle.labels(),
            handle.supervisor(),
            rig.gateway().providers(),
        );
        assert!(matches!(
            rig.world
                .session
                .integrate(&handle.name(), &CancellationToken::new())
                .await,
            Err(IntegrateError::Gateway(GatewayError::Protocol { .. }))
        ));
        assert_eq!(head, handle.git(&["rev-parse", "HEAD"]).await?);
        assert_eq!(
            before,
            (
                handle.files(),
                handle.labels(),
                handle.supervisor(),
                rig.gateway().providers()
            )
        );
        assert_eq!(handle.attachments(), 0);
        assert!(handle.shells().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn direct_instance_operations_refuse_missing_malformed_empty_and_duplicate_manifests()
-> TestResult {
    guard(async {
        let rig = configured().await?;
        let target = ready(
            run(
                &rig,
                rig.request(37)?.instances(members(Agent::ClaudeCode)?),
            )
            .await?,
        )?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
        let manifest = handle.root().join(".jackin/agents.json");
        let item = members(Agent::ClaudeCode)?.as_slice()[0].clone();
        let duplicate = serde_json::to_vec(&vec![item.clone(), item])?;
        for bytes in [
            None,
            Some(b"not json".to_vec()),
            Some(b"[]".to_vec()),
            Some(duplicate),
        ] {
            match bytes {
                Some(bytes) => std::fs::write(&manifest, bytes)?,
                None => std::fs::remove_file(&manifest)?,
            }
            let before = (
                handle.files(),
                handle.labels(),
                handle.supervisor(),
                rig.gateway().providers(),
            );
            assert!(matches!(
                rig.world
                    .session
                    .attach(
                        AttachRequest::viewer(handle.name(), size()?),
                        &CancellationToken::new()
                    )
                    .await,
                Err(AttachError::Gateway(GatewayError::Protocol { .. }))
            ));
            assert!(matches!(
                rig.world
                    .session
                    .integrate(&handle.name(), &CancellationToken::new())
                    .await,
                Err(IntegrateError::Gateway(GatewayError::Protocol { .. }))
            ));
            assert_eq!(
                before,
                (
                    handle.files(),
                    handle.labels(),
                    handle.supervisor(),
                    rig.gateway().providers()
                )
            );
            assert_eq!(handle.attachments(), 0);
            assert!(handle.shells().is_empty());
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn integrate_merges_the_secondary_branch_into_the_persisted_nonlexical_lead() -> TestResult {
    guard(async {
        let rig = configured().await?;
        let target = ready(
            run(
                &rig,
                rig.request(38)?.instances(members(Agent::ClaudeCode)?),
            )
            .await?,
        )?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
        handle.git(&["config", "user.name", "jackin-test"]).await?;
        handle
            .git(&["config", "user.email", "test@example.invalid"])
            .await?;
        let secondary = handle.root().join("work/codex/project");
        std::fs::write(
            secondary.join("secondary-result.txt"),
            b"secondary result\n",
        )?;
        handle
            .git(&["-C", "../work/codex/project", "add", "secondary-result.txt"])
            .await?;
        handle
            .git(&[
                "-C",
                "../work/codex/project",
                "commit",
                "-q",
                "-m",
                "test: add secondary result",
            ])
            .await?;
        let secondary_head = handle
            .git(&["-C", "../work/codex/project", "rev-parse", "HEAD"])
            .await?;
        let merged = rig
            .world
            .session
            .integrate(&target.sandbox, &CancellationToken::new())
            .await?;
        assert_eq!(merged, ["agent/codex"]);
        assert_eq!(handle.git(&["rev-parse", "HEAD"]).await?, secondary_head);
        assert_eq!(
            std::fs::read(handle.root().join("project/secondary-result.txt"))?,
            b"secondary result\n"
        );
        assert!(
            handle
                .git(&["branch", "--list", "agent/claude"])
                .await?
                .trim()
                .is_empty()
        );
        std::fs::write(secondary.join(" "), b"secondary conflict\n")?;
        handle
            .git(&["-C", "../work/codex/project", "add", "--", " "])
            .await?;
        handle
            .git(&[
                "-C",
                "../work/codex/project",
                "commit",
                "-qam",
                "test: secondary conflict",
            ])
            .await?;
        handle
            .commit_in_project(" ", b"lead conflict\n", "test: lead conflict")
            .await?;
        let lead_head = handle.git(&["rev-parse", "HEAD"]).await?;
        let result = rig
            .world
            .session
            .integrate(&target.sandbox, &CancellationToken::new())
            .await;
        assert!(
            matches!(&result, Err(IntegrateError::Conflict { branch }) if branch == "agent/codex"),
            "{result:?}"
        );
        assert_eq!(handle.git(&["rev-parse", "HEAD"]).await?, lead_head);
        assert!(
            !handle
                .git(&["diff", "--name-only", "-z", "--diff-filter=U"])
                .await?
                .is_empty()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn integrate_reports_a_missing_secondary_branch_as_a_git_failure() -> TestResult {
    guard(async {
        let rig = configured().await?;
        let target = ready(
            run(
                &rig,
                rig.request(41)?.instances(members(Agent::ClaudeCode)?),
            )
            .await?,
        )?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
        handle
            .git(&["update-ref", "-d", "refs/heads/agent/codex"])
            .await?;
        let head = handle.git(&["rev-parse", "HEAD"]).await?;
        let error = rig
            .world
            .session
            .integrate(&target.sandbox, &CancellationToken::new())
            .await
            .expect_err("a missing secondary branch must fail");
        assert!(
            matches!(
                &error,
                IntegrateError::Gateway(GatewayError::Protocol { message })
                    if message.starts_with("git merge agent/codex exited ")
                        && message
                            .rsplit_once(": ")
                            .is_some_and(|(_, output)| !output.trim().is_empty())
            ),
            "{error:?}"
        );
        assert_eq!(handle.git(&["rev-parse", "HEAD"]).await?, head);
        assert!(
            handle
                .git(&["diff", "--name-only", "--diff-filter=U"])
                .await?
                .trim()
                .is_empty()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn direct_instance_operations_refuse_delimiter_and_control_models_even_with_matching_hash()
-> TestResult {
    guard(async {
        let rig = configured().await?;
        let target = ready(
            run(
                &rig,
                rig.request(39)?.instances(members(Agent::ClaudeCode)?),
            )
            .await?,
        )?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
        for model in ["gpt,codex:codex:work:bogus", "gpt\ncontrol"] {
            let mut items = members(Agent::ClaudeCode)?.as_slice().to_vec();
            items[1].model = model.to_owned();
            let invalid = AgentInstances::new(items)?;
            handle.add_label(jackin_core::AGENTS_LABEL, &invalid.agents_label())?;
            std::fs::write(
                handle.root().join(".jackin/agents.json"),
                serde_json::to_vec(invalid.as_slice())?,
            )?;
            let before = (
                handle.files(),
                handle.labels(),
                handle.supervisor(),
                rig.gateway().providers(),
            );
            assert!(matches!(
                rig.world
                    .session
                    .attach(
                        AttachRequest::viewer(handle.name(), size()?),
                        &CancellationToken::new()
                    )
                    .await,
                Err(AttachError::Gateway(GatewayError::Protocol { .. }))
            ));
            assert!(matches!(
                rig.world
                    .session
                    .integrate(&handle.name(), &CancellationToken::new())
                    .await,
                Err(IntegrateError::Gateway(GatewayError::Protocol { .. }))
            ));
            assert_eq!(
                before,
                (
                    handle.files(),
                    handle.labels(),
                    handle.supervisor(),
                    rig.gateway().providers()
                )
            );
            assert_eq!(handle.attachments(), 0);
            assert!(handle.shells().is_empty());
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn public_instance_reader_canonicalizes_secondaries_but_preserves_the_lead_identity()
-> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let target = ready(rig.launch_observed(40).await?)?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
        let mut items = members(Agent::ClaudeCode)?.as_slice().to_vec();
        let grok = jackin_core::lookup_tuple(Agent::GrokBuild, Provider::Grok)?;
        items.push(AgentInstance::new(
            grok.agent,
            grok.provider,
            AccountName::parse("work")?,
            grok.default_model.to_owned(),
        ));
        let identity = AgentInstances::new(items.clone())?;
        handle.add_label(jackin_core::AGENTS_LABEL, &identity.agents_label())?;
        let expected = vec![
            Agent::ClaudeCode.as_str().to_owned(),
            Agent::Codex.as_str().to_owned(),
            Agent::GrokBuild.as_str().to_owned(),
        ];
        let mut permuted = items.clone();
        permuted.swap(1, 2);
        let mut changed_lead = items;
        changed_lead.swap(0, 1);
        for (bytes, expected) in [
            (identity.agents_json().into_bytes(), Some(expected.clone())),
            (serde_json::to_vec(&permuted)?, Some(expected)),
            (serde_json::to_vec(&changed_lead)?, None),
        ] {
            std::fs::write(handle.root().join(".jackin/agents.json"), bytes)?;
            let before = (
                handle.files(),
                handle.labels(),
                rig.gateway().providers(),
                handle.supervisor(),
                handle.main_input(),
                handle.shells(),
                handle.attachments(),
            );
            let names = rig.world.session.sandbox_agent_names(&target.sandbox).await;
            match expected {
                Some(expected) => assert_eq!(names?, expected),
                None => assert!(matches!(names, Err(GatewayError::Protocol { .. }))),
            }
            assert_eq!(
                before,
                (
                    handle.files(),
                    handle.labels(),
                    rig.gateway().providers(),
                    handle.supervisor(),
                    handle.main_input(),
                    handle.shells(),
                    handle.attachments(),
                )
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancelled_attach_stops_the_pre_attach_kill_at_once() -> TestResult {
    guard(async {
        let rig = configured().await?;
        let request = rig.request(42)?.instances(members(Agent::ClaudeCode)?);
        let target = ready(run(&rig, request).await?)?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
        let live = LiveInstance::spawn()?;
        std::fs::write(
            handle.root().join(".jackin/run/codex/instance.pid"),
            live.record().await?,
        )?;
        rig.gateway().script_program(
            &SandboxPath::from_static("/usr/bin/kill"),
            ProgramScript {
                exit_code: 0,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
        );
        rig.gateway().script_program(
            &SandboxPath::from_static("/usr/bin/ps"),
            ProgramScript {
                exit_code: 1,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
        );
        let cancel = CancellationToken::new();
        cancel.cancel();
        let mut request = AttachRequest::owner(target.clone(), size()?);
        request.agent = Some(Agent::Codex);
        let refused = rig.world.session.attach(request, &cancel).await;
        assert!(
            matches!(
                refused,
                Err(AttachError::Gateway(GatewayError::Cancelled { .. }))
            ),
            "{refused:?}"
        );
        assert!(handle.shells().is_empty());
        assert!(
            handle
                .runs(&SandboxPath::from_static("/usr/bin/kill"))
                .is_empty()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn secondary_attach_stops_the_detached_instance_and_carries_the_launch_env() -> TestResult {
    guard(async {
        let rig = configured().await?;
        let request = rig.request(36)?.instances(members(Agent::ClaudeCode)?);
        let target = ready(run(&rig, request).await?)?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
        let live = LiveInstance::spawn()?;
        std::fs::write(
            handle.root().join(".jackin/run/codex/instance.pid"),
            live.record().await?,
        )?;
        rig.gateway().script_program(
            &SandboxPath::from_static("/usr/bin/kill"),
            ProgramScript {
                exit_code: 0,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
        );
        rig.gateway().script_program(
            &SandboxPath::from_static("/usr/bin/ps"),
            ProgramScript {
                exit_code: 1,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
        );
        let mut request = AttachRequest::owner(target.clone(), size()?);
        request.agent = Some(Agent::Codex);
        rig.world
            .session
            .attach(request, &CancellationToken::new())
            .await?;
        let pid = live.pid().to_string();
        let signalled = handle.runs(&SandboxPath::from_static("/usr/bin/kill"));
        assert_eq!(signalled.len(), 1, "{signalled:?}");
        assert_eq!(signalled[0].args, vec!["-TERM", &pid]);
        let probed = handle.runs(&SandboxPath::from_static("/usr/bin/ps"));
        assert_eq!(probed.len(), 1, "{probed:?}");
        assert_eq!(probed[0].args, vec!["-p", &pid]);
        assert_eq!(probed[0].exit_code, 1);
        let queries = identity_queries(&handle);
        assert_eq!(queries.len(), 1, "{queries:?}");
        assert!(queries[0].iter().any(|arg| arg == &pid), "{queries:?}");
        assert!(
            !handle
                .root()
                .join(".jackin/run/codex/instance.pid")
                .exists()
        );
        let shells = handle.shells();
        assert_eq!(shells.len(), 1);
        assert_eq!(
            shells[0]
                .env
                .get(&EnvName::parse("GREETING")?)
                .map(String::as_str),
            Some("hello")
        );
        assert_eq!(
            shells[0]
                .env
                .get(&EnvName::parse("HOME")?)
                .map(String::as_str),
            Some("/sandbox/.jackin/homes/codex")
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn secondary_attach_refuses_when_the_detached_instance_cannot_be_stopped() -> TestResult {
    guard(async {
        let rig = configured().await?;
        let request = rig.request(37)?.instances(members(Agent::ClaudeCode)?);
        let target = ready(run(&rig, request).await?)?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
        let live = LiveInstance::spawn()?;
        std::fs::write(
            handle.root().join(".jackin/run/codex/instance.pid"),
            live.record().await?,
        )?;
        rig.gateway().script_program(
            &SandboxPath::from_static("/usr/bin/kill"),
            ProgramScript {
                exit_code: 2,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
        );
        let mut request = AttachRequest::owner(target.clone(), size()?);
        request.agent = Some(Agent::Codex);
        let refused = rig
            .world
            .session
            .attach(request, &CancellationToken::new())
            .await;
        assert!(refused.is_err(), "{refused:?}");
        assert!(handle.shells().is_empty());
        assert!(
            handle
                .root()
                .join(".jackin/run/codex/instance.pid")
                .exists()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn secondary_attach_waits_for_the_detached_instance_to_exit_before_the_replacement()
-> TestResult {
    guard(async {
        let rig = configured().await?;
        let request = rig.request(38)?.instances(members(Agent::ClaudeCode)?);
        let target = ready(run(&rig, request).await?)?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
        let live = LiveInstance::spawn()?;
        std::fs::write(
            handle.root().join(".jackin/run/codex/instance.pid"),
            live.record().await?,
        )?;
        for (program, code) in [("/usr/bin/kill", 0), ("/usr/bin/ps", 0)] {
            rig.gateway().script_program(
                &SandboxPath::from_static(program),
                ProgramScript {
                    exit_code: code,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                },
            );
        }
        let mut request = AttachRequest::owner(target.clone(), size()?);
        request.agent = Some(Agent::Codex);
        let refused = rig
            .world
            .session
            .attach(request, &CancellationToken::new())
            .await;
        assert!(refused.is_err(), "{refused:?}");
        assert!(handle.shells().is_empty());
        let probed = handle.runs(&SandboxPath::from_static("/usr/bin/ps"));
        assert!(probed.len() > 1, "{probed:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn secondary_attach_refuses_to_signal_a_stale_instance_identity() -> TestResult {
    guard(async {
        let rig = configured().await?;
        let request = rig.request(39)?.instances(members(Agent::ClaudeCode)?);
        let target = ready(run(&rig, request).await?)?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
        let pid = stale_pid()?;
        std::fs::write(
            handle.root().join(".jackin/run/codex/instance.pid"),
            format!("{pid} retired"),
        )?;
        let mut request = AttachRequest::owner(target.clone(), size()?);
        request.agent = Some(Agent::Codex);
        rig.world
            .session
            .attach(request, &CancellationToken::new())
            .await?;
        let signalled = handle.runs(&SandboxPath::from_static("/usr/bin/kill"));
        assert!(signalled.is_empty(), "{signalled:?}");
        let queries = identity_queries(&handle);
        assert_eq!(queries.len(), 1, "{queries:?}");
        assert!(
            queries[0].iter().any(|arg| arg == &pid.to_string()),
            "{queries:?}"
        );
        assert!(
            !handle
                .root()
                .join(".jackin/run/codex/instance.pid")
                .exists()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn secondary_attach_refuses_to_signal_a_reused_instance_identity() -> TestResult {
    guard(async {
        let rig = configured().await?;
        let request = rig.request(40)?.instances(members(Agent::ClaudeCode)?);
        let target = ready(run(&rig, request).await?)?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
        let mut taken = LiveInstance::spawn()?;
        let mut sibling = LiveInstance::spawn()?;
        std::fs::write(
            handle.root().join(".jackin/run/codex/instance.pid"),
            format!("{} retired", taken.pid()),
        )?;
        let mut request = AttachRequest::owner(target.clone(), size()?);
        request.agent = Some(Agent::Codex);
        rig.world
            .session
            .attach(request, &CancellationToken::new())
            .await?;
        let signalled = handle.runs(&SandboxPath::from_static("/usr/bin/kill"));
        assert!(signalled.is_empty(), "{signalled:?}");
        assert!(
            taken.running(),
            "the process that took the pid was signalled"
        );
        assert!(sibling.running(), "a sibling process was signalled");
        assert!(
            !handle
                .root()
                .join(".jackin/run/codex/instance.pid")
                .exists()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn secondary_attach_refuses_a_record_that_names_no_process() -> TestResult {
    guard(async {
        let rig = configured().await?;
        let request = rig.request(41)?.instances(members(Agent::ClaudeCode)?);
        let target = ready(run(&rig, request).await?)?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox")?;
        std::fs::write(handle.root().join(".jackin/run/codex/instance.pid"), b"0")?;
        let mut request = AttachRequest::owner(target.clone(), size()?);
        request.agent = Some(Agent::Codex);
        let refused = rig
            .world
            .session
            .attach(request, &CancellationToken::new())
            .await;
        assert!(
            matches!(
                refused,
                Err(AttachError::Gateway(GatewayError::Protocol { ref message }))
                    if message.contains("not a process")
            ),
            "{refused:?}"
        );
        let signalled = handle.runs(&SandboxPath::from_static("/usr/bin/kill"));
        assert!(signalled.is_empty(), "{signalled:?}");
        assert!(identity_queries(&handle).is_empty());
        assert!(
            handle
                .root()
                .join(".jackin/run/codex/instance.pid")
                .exists()
        );
        assert!(handle.shells().is_empty());
        Ok(())
    })
    .await
}
