// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, Agent, AgentInstance, AgentInstances, ConfigEdit, CredentialSource,
    FileMode, HostPath, Provider, ProviderName, SandboxPath, Secret, sandbox_layout,
};
use jackin_openshell::{
    EndpointProtocol, Gateway, GatewayError, HostPattern, NetworkRule, ProviderMaterial,
    ProviderProfile, ProviderSpec, RuleEndpoint, RuleKey, TlsMode,
};
use jackin_session::LaunchStep;
use jackin_testkit::{Fault, ProgramScript, Rig, Trigger};
use tokio_util::sync::CancellationToken;

use crate::support::{TestResult, failure, guard, launch_id, multi_agent_rig, ready, running};

const OMP_ACCOUNT: &str = "zai-work";
const KIMI_ACCOUNT: &str = "kimi-work";

async fn seed_kimi_provider(rig: &mut Rig) -> Result<(), Box<dyn std::error::Error>> {
    let path = HostPath::parse(&rig.home.as_path().join("kimi.key"))?;
    rig.world
        .fs
        .write_new(&path, b"test-key", FileMode(0o600))
        .await?;
    rig.world
        .config
        .edit(vec![ConfigEdit::AddAccount(Account::new(
            AccountName::parse(KIMI_ACCOUNT)?,
            Provider::Kimi,
            CredentialSource::File { path },
        )?)])
        .await?;
    rig.config = rig.world.config.load().await?;
    let tuple = jackin_core::lookup_tuple(Agent::Omp, Provider::Kimi)?;
    let profile = ProviderProfile::parse(jackin_core::tuple_profile_asset(tuple))?;
    rig.gateway().ensure_profile(&profile).await?;
    rig.gateway()
        .put_provider(&ProviderSpec {
            name: ProviderName::for_account(&AccountName::parse(KIMI_ACCOUNT)?, Agent::Omp),
            profile: profile.id,
            keys: jackin_core::CredentialKeys {
                access_token: "KIMI_API_KEY",
                refresh_token: "",
                account_id: "",
                id_token: "",
                token_url: "",
            },
            material: ProviderMaterial::StaticKey {
                key: Secret::new("test-key".to_owned()),
            },
        })
        .await?;
    Ok(())
}

fn split_host_instances() -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
        AgentInstance::new(
            Agent::ClaudeCode,
            Provider::Zai,
            AccountName::parse("work")?,
            "glm-5.3".to_owned(),
        ),
        AgentInstance::new(
            Agent::Omp,
            Provider::Kimi,
            AccountName::parse(KIMI_ACCOUNT)?,
            "kimi-for-coding".to_owned(),
        ),
    ])?)
}

fn marker_instances() -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
        AgentInstance::new(
            Agent::Codex,
            Provider::Zai,
            AccountName::parse("work")?,
            "glm-5.3".to_owned(),
        ),
        AgentInstance::new(
            Agent::Omp,
            Provider::Kimi,
            AccountName::parse(KIMI_ACCOUNT)?,
            "kimi-for-coding".to_owned(),
        ),
        AgentInstance::new(
            Agent::ClaudeCode,
            Provider::Zai,
            AccountName::parse(OMP_ACCOUNT)?,
            "glm-5.3".to_owned(),
        ),
    ])?)
}

async fn seed_omp_provider(rig: &mut Rig) -> Result<(), Box<dyn std::error::Error>> {
    let path = HostPath::parse(&rig.home.as_path().join("zai.key"))?;
    rig.world
        .fs
        .write_new(&path, b"test-key", FileMode(0o600))
        .await?;
    rig.world
        .config
        .edit(vec![ConfigEdit::AddAccount(Account::new(
            AccountName::parse(OMP_ACCOUNT)?,
            Provider::Zai,
            CredentialSource::File { path },
        )?)])
        .await?;
    rig.config = rig.world.config.load().await?;
    let tuple = jackin_core::lookup_tuple(Agent::Omp, Provider::Zai)?;
    let profile = ProviderProfile::parse(jackin_core::tuple_profile_asset(tuple))?;
    rig.gateway().ensure_profile(&profile).await?;
    rig.gateway()
        .put_provider(&ProviderSpec {
            name: ProviderName::for_account(&AccountName::parse(OMP_ACCOUNT)?, Agent::Omp),
            profile: profile.id,
            keys: jackin_core::CredentialKeys {
                access_token: "ZAI_API_KEY",
                refresh_token: "",
                account_id: "",
                id_token: "",
                token_url: "",
            },
            material: ProviderMaterial::StaticKey {
                key: Secret::new("test-key".to_owned()),
            },
        })
        .await?;
    Ok(())
}

fn instances() -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
        AgentInstance::new(
            Agent::ClaudeCode,
            Provider::Zai,
            AccountName::parse("work")?,
            "glm-5.3".to_owned(),
        ),
        AgentInstance::new(
            Agent::Omp,
            Provider::Zai,
            AccountName::parse(OMP_ACCOUNT)?,
            "glm-5.3".to_owned(),
        ),
    ])?)
}

fn codex_instances() -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
        AgentInstance::new(
            Agent::Codex,
            Provider::Zai,
            AccountName::parse("work")?,
            "glm-5.3".to_owned(),
        ),
        AgentInstance::new(
            Agent::ClaudeCode,
            Provider::Zai,
            AccountName::parse(OMP_ACCOUNT)?,
            "glm-5.3".to_owned(),
        ),
    ])?)
}

#[tokio::test]
async fn two_instance_launch_leaves_non_lead_ready_and_supervises_only_lead()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let mut rig = multi_agent_rig().await?;
        seed_omp_provider(&mut rig).await?;
        let request = rig.request(7)?.instances(instances()?);
        let outcome = rig.run(request, CancellationToken::new()).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        assert_eq!(handle.supervisor(), Some(running(1)?));
        let sh = SandboxPath::from_static("/bin/sh");
        let runs = handle.runs(&sh);
        assert_eq!(
            runs.iter()
                .filter(|run| run
                    .args
                    .iter()
                    .any(|arg| arg.contains("/sandbox/.jackin/run/omp/output")))
                .count(),
            0
        );
        assert!(
            !runs.iter().any(|run| run
                .args
                .iter()
                .any(|arg| arg.contains("setsid") || arg.contains("script -qec"))),
            "{runs:?}"
        );
        assert!(handle.root().join(".jackin/run/omp").is_dir());
        assert!(handle.root().join(".jackin/task/status").is_dir());
        assert!(handle.root().join(".jackin/task").is_dir());
        let lead_starts = runs
            .iter()
            .filter(|run| {
                run.args
                    .iter()
                    .any(|arg| arg.contains("/sandbox/.jackin/run/claude/output"))
            })
            .count();
        assert_eq!(lead_starts, 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn restart_omp_stops_omp_only_and_starts_no_process() -> Result<(), Box<dyn std::error::Error>>
{
    guard(async {
        let mut rig = multi_agent_rig().await?;
        seed_omp_provider(&mut rig).await?;
        let request = rig.request(9)?.instances(instances()?);
        let outcome = rig.run(request, CancellationToken::new()).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let sh = SandboxPath::from_static("/bin/sh");
        let count = |handle: &jackin_testkit::SandboxHandle, agent: &str| {
            handle
                .runs(&sh)
                .iter()
                .filter(|run| {
                    run.args
                        .iter()
                        .any(|arg| arg.contains(&format!("/sandbox/.jackin/run/{agent}/output")))
                })
                .count()
        };
        assert_eq!(count(&handle, "omp"), 0);
        let kill = SandboxPath::from_static("/usr/bin/kill");
        rig.gateway().script_program(
            &kill,
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
        let live = crate::support::LiveInstance::spawn()?;
        std::fs::write(
            handle.root().join(".jackin/run/omp/instance.pid"),
            live.record().await?,
        )?;
        let stops_before = handle.runs(&kill).len();
        let session = rig.session_with(crate::support::limits(30, 20));
        session
            .restart_instance(&target.sandbox, Agent::Omp, &CancellationToken::new())
            .await?;
        assert_eq!(count(&handle, "omp"), 0);
        assert_eq!(count(&handle, "claude"), 0);
        let stops = handle.runs(&kill);
        assert_eq!(stops.len(), stops_before + 1);
        assert!(
            stops
                .iter()
                .any(|run| run.args == vec!["-TERM", &live.pid().to_string()]),
            "{stops:?}"
        );
        let probed = handle.runs(&SandboxPath::from_static("/usr/bin/ps"));
        assert!(
            probed
                .iter()
                .any(|run| run.args == vec!["-p", &live.pid().to_string()]),
            "{probed:?}"
        );
        assert!(!handle.root().join(".jackin/run/omp/instance.pid").exists());
        assert!(!handle.runs(&sh).iter().any(|run| {
            run.args
                .iter()
                .any(|arg| arg.contains("setsid") || arg.contains("script"))
        }));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn two_instance_launch_writes_codex_env_like_single_instance()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let mut rig = multi_agent_rig().await?;
        seed_omp_provider(&mut rig).await?;
        let request = rig.request(8)?.instances(codex_instances()?);
        let outcome = rig.run(request, CancellationToken::new()).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let path = sandbox_layout::env_file(&launch_id(8));
        let env = handle.file(&path).ok_or("no codex env")?;
        assert!(!env.is_empty());
        assert_eq!(handle.file_mode(&path), Some(FileMode(0o600)));
        let single = multi_agent_rig().await?;
        ready(single.launch_observed(1).await?)?;
        let single_handle = single.handle(1).await?;
        let single_path = sandbox_layout::env_file(&launch_id(1));
        assert_eq!(
            env,
            single_handle.file(&single_path).ok_or("no single env")?
        );
        assert_eq!(
            handle.file_mode(&path),
            single_handle.file_mode(&single_path)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn failed_codex_provider_publication_cleans_up_only_codex_and_preserves_sibling_providers()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let mut rig = multi_agent_rig().await?;
        let work = AccountName::parse("work")?;
        let path = HostPath::parse(&rig.world.home().as_path().join("zai.key"))?;
        rig.world
            .fs
            .write_new(&path, b"ZAI-FIXTURE-KEY", FileMode(0o600))
            .await?;
        let account = Account::new(work.clone(), Provider::Zai, CredentialSource::File { path })?;
        let edits = vec![
            ConfigEdit::RemoveAccountClearingBindings(work),
            ConfigEdit::AddAccount(account.clone()),
        ];
        for edit in &edits {
            rig.config.apply(edit.clone())?;
        }
        rig.world.config.edit(edits).await?;
        rig.world
            .session
            .sync_account(&account, Agent::Omp, &CancellationToken::new())
            .await?;
        let other = Account::new(
            AccountName::parse("other")?,
            Provider::Zai,
            account.credential().clone(),
        )?;
        rig.world
            .config
            .edit(vec![ConfigEdit::AddAccount(other.clone())])
            .await?;
        rig.world
            .session
            .sync_account(&other, Agent::Omp, &CancellationToken::new())
            .await?;
        let omp = ProviderName::for_account(account.name(), Agent::Omp);
        let codex = ProviderName::for_account(account.name(), Agent::Codex);
        let other_provider = ProviderName::for_account(other.name(), Agent::Omp);
        let original_omp = rig.gateway().provider(&omp);
        let original_other = rig.gateway().provider(&other_provider);
        rig.gateway().inject(Fault::PutProviderFailsAfterWrite {
            error: GatewayError::Unreachable {
                endpoint: "gateway".to_owned(),
                message: "connection reset".to_owned(),
            },
        });
        let members = AgentInstances::new(vec![
            AgentInstance::new(
                Agent::Codex,
                Provider::Zai,
                account.name().clone(),
                "glm-5.3".to_owned(),
            ),
            AgentInstance::new(
                Agent::ClaudeCode,
                Provider::Zai,
                account.name().clone(),
                "glm-5.3".to_owned(),
            ),
        ])?;
        let outcome = rig
            .run(
                rig.request(42)?.instances(members),
                CancellationToken::new(),
            )
            .await?;

        let failed = outcome.result.err().ok_or("the launch went through")?;
        assert_eq!(failed.step, LaunchStep::Provider);
        assert!(rig.gateway().provider(&codex).is_none());
        assert_eq!(rig.gateway().provider(&omp), original_omp);
        assert_eq!(rig.gateway().provider(&other_provider), original_other);
        assert!(rig.gateway().sandboxes().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn t18_lead_and_t14_non_lead_write_the_claude_instance_model() -> TestResult {
    guard(async {
        let role = jackin_testkit::RoleFixture::new("sentinel")?.manifest_text(
            "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[agents.codex]\ninstructions = \"instructions/codex.md\"\n\n[agents.claude]\ninstructions = \"instructions/claude.md\"\n".to_owned(),
        );
        let rig = super::support::builder().await?.role(role).build().await?;
        let instances = AgentInstances::new(vec![
            AgentInstance::new(
                Agent::Codex,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
            AgentInstance::new(
                Agent::ClaudeCode,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
        ])?;
        let outcome = rig
            .run(
                rig.request(44)?.instances(instances),
                CancellationToken::new(),
            )
            .await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let path = SandboxPath::from_static(
            "/sandbox/.jackin/homes/claude/.claude/settings.json",
        );
        let bytes = handle.file(&path).ok_or("no non-lead Claude settings")?;
        let rendered = String::from_utf8(bytes)?;
        let value: serde_json::Value = serde_json::from_str(&rendered)?;
        assert_eq!(
            value,
            serde_json::json!({"model":"glm-5.3","env":{"ANTHROPIC_BASE_URL":"https://api.z.ai/api/anthropic"}})
        );
        assert_eq!(handle.file_mode(&path), Some(FileMode(0o600)));
        for forbidden in [
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_API_KEY",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "placeholder",
        ] {
            assert!(!rendered.contains(forbidden), "{forbidden}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn inspect_reports_a_live_rule_on_the_provider_host_of_every_instance()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let mut rig = multi_agent_rig().await?;
        seed_kimi_provider(&mut rig).await?;
        let request = rig.request(11)?.instances(split_host_instances()?);
        let target = ready(rig.run(request, CancellationToken::new()).await?)?;
        rig.world.gateway.add_live_rule(
            &target.sandbox,
            NetworkRule {
                key: RuleKey::provider_rule(&ProviderName::parse("other")?),
                endpoints: vec![RuleEndpoint {
                    host: HostPattern::parse("api.z.ai")?,
                    port: 443,
                    protocol: EndpointProtocol::Tcp,
                    tls: TlsMode::Default,
                }],
                binaries: Vec::new(),
            },
        );
        rig.world.gateway.add_live_rule(
            &target.sandbox,
            NetworkRule {
                key: RuleKey::provider_rule(&ProviderName::parse("kimiother")?),
                endpoints: vec![RuleEndpoint {
                    host: HostPattern::parse("api.kimi.com")?,
                    port: 443,
                    protocol: EndpointProtocol::Tcp,
                    tls: TlsMode::Default,
                }],
                binaries: Vec::new(),
            },
        );
        let inspected = rig.world.session.inspect(&target.sandbox).await?;
        let rules: Vec<(&str, &str)> = inspected
            .closure_violations
            .iter()
            .map(|view| (view.rule.as_str(), view.host.as_str()))
            .collect();
        assert_eq!(
            rules,
            vec![
                ("_provider_kimiother", "api.kimi.com"),
                ("_provider_other", "api.z.ai")
            ]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn inspect_covers_the_instances_after_their_local_accounts_are_removed()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let mut rig = multi_agent_rig().await?;
        seed_kimi_provider(&mut rig).await?;
        let request = rig.request(14)?.instances(split_host_instances()?);
        let target = ready(rig.run(request, CancellationToken::new()).await?)?;
        rig.world
            .config
            .edit(vec![
                ConfigEdit::RemoveAccountClearingBindings(AccountName::parse("work")?),
                ConfigEdit::RemoveAccountClearingBindings(AccountName::parse(KIMI_ACCOUNT)?),
            ])
            .await?;
        rig.world.gateway.add_live_rule(
            &target.sandbox,
            NetworkRule {
                key: RuleKey::provider_rule(&ProviderName::parse("other")?),
                endpoints: vec![RuleEndpoint {
                    host: HostPattern::parse("api.z.ai")?,
                    port: 443,
                    protocol: EndpointProtocol::Tcp,
                    tls: TlsMode::Default,
                }],
                binaries: Vec::new(),
            },
        );
        rig.world.gateway.add_live_rule(
            &target.sandbox,
            NetworkRule {
                key: RuleKey::provider_rule(&ProviderName::parse("kimiother")?),
                endpoints: vec![RuleEndpoint {
                    host: HostPattern::parse("api.kimi.com")?,
                    port: 443,
                    protocol: EndpointProtocol::Tcp,
                    tls: TlsMode::Default,
                }],
                binaries: Vec::new(),
            },
        );
        let inspected = rig.world.session.inspect(&target.sandbox).await?;
        let rules: Vec<(&str, &str)> = inspected
            .closure_violations
            .iter()
            .map(|view| (view.rule.as_str(), view.host.as_str()))
            .collect();
        assert_eq!(
            rules,
            vec![
                ("_provider_kimiother", "api.kimi.com"),
                ("_provider_other", "api.z.ai")
            ]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_upload_marker_waits_for_the_worktrees_of_every_instance()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let mut rig = multi_agent_rig().await?;
        seed_omp_provider(&mut rig).await?;
        seed_kimi_provider(&mut rig).await?;
        let program = Trigger::Program(SandboxPath::from_static("/bin/sh"));
        rig.gateway().inject(Fault::HoldAt {
            trigger: program.clone(),
        });
        rig.gateway().inject(Fault::HoldAt {
            trigger: program.clone(),
        });
        let launching = rig.run(
            rig.request(12)?.instances(marker_instances()?),
            CancellationToken::new(),
        );
        let releasing = async {
            assert!(rig.gateway().until_held(&program).await);
            rig.gateway().release(&program);
            assert!(rig.gateway().until_held(&program).await);
            rig.gateway().inject(Fault::FailAt {
                trigger: program.clone(),
                error: GatewayError::Protocol {
                    message: "injected".to_owned(),
                },
            });
            rig.gateway().release(&program);
        };
        let (outcome, ()) = tokio::join!(launching, releasing);
        let failed = failure(outcome?)?;
        assert_eq!(failed.step, LaunchStep::Upload);
        let handle = rig
            .gateway()
            .sandbox(&failed.sandbox.ok_or("sandbox")?)
            .ok_or("sandbox handle")?;
        assert!(
            handle
                .file(&sandbox_layout::uploaded_marker(&launch_id(12)))
                .is_none()
        );
        let first = handle.root().join("work/omp/project");
        assert!(first.join(".git").is_file(), "{first:?}");
        assert!(!handle.root().join("work/claude/project").exists());
        std::fs::write(first.join("uncommitted.txt"), b"kept")?;
        let outcome = rig
            .run(
                rig.request(12)?.instances(marker_instances()?),
                CancellationToken::new(),
            )
            .await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        assert!(
            handle
                .file(&sandbox_layout::uploaded_marker(&launch_id(12)))
                .is_some()
        );
        assert!(first.join(".git").is_file());
        assert!(first.join("uncommitted.txt").is_file());
        assert!(handle.root().join("work/claude/project/.git").is_file());
        let registered = handle.git(&["worktree", "list", "--porcelain"]).await?;
        assert!(registered.contains("work/omp/project"), "{registered}");
        let branch = handle
            .git(&["-C", "../work/omp/project", "symbolic-ref", "HEAD"])
            .await?;
        assert_eq!(branch.trim(), "refs/heads/agent/omp");
        let usable = handle
            .git(&["-C", "../work/omp/project", "rev-parse", "HEAD"])
            .await?;
        assert!(!usable.trim().is_empty());
        let state = handle
            .git(&["-C", "../work/omp/project", "status", "--porcelain"])
            .await?;
        assert!(state.contains("uncommitted.txt"), "{state}");
        Ok(())
    })
    .await
}
