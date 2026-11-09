// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;

use jackin_core::{
    Account, AccountName, Agent, Config, ConfigEdit, CredentialSource, HostPath, ProjectRoot,
    Provider, ProviderName, RoleName, SandboxPath,
};
use jackin_image::ensure_image;
use jackin_openshell::{
    EndpointProtocol, Gateway, HostPattern, NetworkRule, RuleEndpoint, RuleKey, TlsMode,
};
use jackin_session::{LaunchError, LaunchFailure, LaunchRequest, LoadedRole, plan_launch};
use jackin_testkit::{Outcome, RepoFixture, Rig, RoleFixture, SandboxHandle, launch_id};
use tokio_util::sync::CancellationToken;

use super::docker_support::{rule_keys, split_failure};
use super::support::{TestResult, guard, launch_rig, ready};

const POLICY_KEY: &str = "jackin.dev/policy";

type Snapshot = (
    String,
    BTreeMap<SandboxPath, Vec<u8>>,
    Vec<String>,
    Vec<ProviderName>,
    usize,
);

async fn snapshot(rig: &Rig) -> Result<Snapshot, Box<dyn Error>> {
    let handle = rig.handle(1).await?;
    Ok((
        format!("{:?}", handle.labels()),
        handle.files(),
        rule_keys(rig, &handle).await?,
        rig.gateway().providers(),
        rig.gateway().sandboxes().len(),
    ))
}

async fn retry(
    rig: &Rig,
    config: &Config,
    loaded: &LoadedRole,
    request: LaunchRequest,
) -> Result<Outcome, Box<dyn Error>> {
    let plan = plan_launch(config, loaded, request)?;
    Ok(rig
        .run_planned(&rig.world.session, plan, CancellationToken::new())
        .await)
}

async fn refused(rig: &Rig, outcome: Outcome) -> Result<String, Box<dyn Error>> {
    let (_, failed): (_, LaunchFailure) = split_failure(outcome)?;
    assert_eq!(failed.step, jackin_session::LaunchStep::Sandbox);
    assert_eq!(failed.sandbox, Some(rig.handle(1).await?.name()));
    assert!(
        matches!(&*failed.error, LaunchError::StaleSandbox { .. }),
        "{:?}",
        failed.error
    );
    let shown = failed.error.to_string();
    assert!(shown.contains("new launch"), "{shown}");
    Ok(shown)
}

fn request_for(
    rig: &Rig,
    project: ProjectRoot,
    role: &str,
) -> Result<LaunchRequest, Box<dyn Error>> {
    Ok(LaunchRequest::new(
        rig.world.gateway.name().clone(),
        project,
        RoleName::parse(role)?,
        launch_id(1),
    )
    .tuple(Agent::Codex, Provider::Zai)
    .model(Some("glm-5.3".to_owned())))
}

fn label(handle: &SandboxHandle, key: &str) -> Option<String> {
    handle
        .labels()
        .iter()
        .find(|(name, _)| name.to_string() == key)
        .map(|(_, value)| value.to_string())
}

struct Other {
    config: Config,
    _home: tempfile::TempDir,
}

fn with_other_account(rig: &Rig, default: bool) -> Result<Other, Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let path = home.path().join("zai.txt");
    std::fs::write(&path, "other-test-key")?;
    let key = HostPath::parse(&std::fs::canonicalize(path)?)?;
    let other = AccountName::parse("other")?;
    let account = Account::new(
        other.clone(),
        Provider::Zai,
        CredentialSource::File { path: key },
    )?;
    let mut config = rig.config.clone();
    config.apply(ConfigEdit::AddAccount(account))?;
    if default {
        config.apply(ConfigEdit::SetDefaultAccount { account: other })?;
    }
    Ok(Other {
        config,
        _home: home,
    })
}

async fn another_account(default: bool) -> TestResult {
    let rig = launch_rig().await?;
    ready(rig.launch_observed(1).await?)?;
    let kept = snapshot(&rig).await?;
    let other = with_other_account(&rig, default)?;
    let request = rig.request(1)?;
    let request = if default {
        request
    } else {
        request.account(Some(AccountName::parse("other")?))
    };
    let shown = refused(
        &rig,
        retry(&rig, &other.config, &rig.loaded, request).await?,
    )
    .await?;
    assert!(shown.contains("jackin.dev/agents"), "{shown}");
    let hash = |account: &str| -> Result<String, Box<dyn Error>> {
        Ok(
            jackin_core::AgentInstances::single(jackin_core::AgentInstance::new(
                Agent::Codex,
                Provider::Zai,
                AccountName::parse(account)?,
                "glm-5.3".to_owned(),
            ))
            .agents_label(),
        )
    };
    let work_hash = hash("work")?;
    let other_hash = hash("other")?;
    assert!(
        shown.contains(&work_hash) && shown.contains(&other_hash),
        "{shown}"
    );
    assert_eq!(snapshot(&rig).await?, kept);

    ready(retry(&rig, &rig.config, &rig.loaded, rig.request(1)?).await?)?;
    assert_eq!(rig.gateway().sandboxes().len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_retry_with_another_account_flag_is_refused_and_changes_nothing() -> TestResult {
    guard(another_account(false)).await
}

#[tokio::test]
async fn a_retry_after_the_default_account_changed_is_refused_and_changes_nothing() -> TestResult {
    guard(another_account(true)).await
}

fn revoke(rig: &Rig) -> Result<(), Box<dyn Error>> {
    let path = rig.loaded.root().as_path().join("policy.yaml");
    let text = std::fs::read_to_string(&path)?.replace("github.com", "revoked-egress.example");
    std::fs::write(path, text)?;
    Ok(())
}

async fn live_policy(rig: &Rig, handle: &SandboxHandle) -> Result<String, Box<dyn Error>> {
    let detail = rig.world.gateway.sandbox_detail(&handle.name()).await?;
    Ok(format!("{:?}", detail.policy.document))
}

#[tokio::test]
async fn a_retry_under_another_policy_is_refused_even_when_the_image_id_is_unchanged() -> TestResult
{
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let first = rig.handle(1).await?;
        let kept = snapshot(&rig).await?;
        let old = rig.final_image()?;

        revoke(&rig)?;
        let changed = rig
            .world
            .roles
            .load_path(rig.loaded.root(), &CancellationToken::new())
            .await?;
        let plan = plan_launch(&rig.config, &changed, rig.request(1)?)?;
        let (events, _receiver) = tokio::sync::mpsc::channel(256);
        let built = ensure_image(
            rig.world.engine.as_ref(),
            plan.image_request(),
            &events,
            &CancellationToken::new(),
        )
        .await?;
        rig.world.engine.pin_id(&built.tag, old.id.clone());

        let shown = refused(
            &rig,
            retry(&rig, &rig.config, &changed, rig.request(1)?).await?,
        )
        .await?;
        assert!(shown.contains(POLICY_KEY), "{shown}");
        assert_eq!(snapshot(&rig).await?, kept);
        let live = live_policy(&rig, &first).await?;
        assert!(live.contains("github.com"), "{live}");
        assert!(!live.contains("revoked-egress.example"), "{live}");

        ready(rig.launch_observed(2).await?)?;
        let second = rig.handle(2).await?;
        let recorded = label(&first, POLICY_KEY);
        assert!(recorded.is_some(), "{:?}", first.labels());
        assert_eq!(label(&second, POLICY_KEY), recorded);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_retry_against_a_sandbox_with_a_label_outside_the_jackin_set_is_refused() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        rig.handle(1)
            .await?
            .add_label("example.com/owner", "someone-else")?;
        let kept = snapshot(&rig).await?;

        let shown = refused(
            &rig,
            retry(&rig, &rig.config, &rig.loaded, rig.request(1)?).await?,
        )
        .await?;
        assert!(shown.contains("example.com/owner"), "{shown}");
        assert!(shown.contains("someone-else"), "{shown}");
        assert!(shown.contains("(none)"), "{shown}");
        assert_eq!(snapshot(&rig).await?, kept);
        Ok(())
    })
    .await
}

fn extra_rule() -> Result<NetworkRule, Box<dyn Error>> {
    Ok(NetworkRule {
        key: RuleKey::provider_rule(&ProviderName::parse("extra")?),
        endpoints: vec![RuleEndpoint {
            host: HostPattern::parse("api.openai.com")?,
            port: 443,
            protocol: EndpointProtocol::Tcp,
            tls: TlsMode::Default,
        }],
        binaries: Vec::new(),
    })
}

#[tokio::test]
async fn a_rule_the_operator_added_after_creation_does_not_make_an_identical_retry_stale()
-> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let name = rig.handle(1).await?.name();
        let rule = extra_rule()?;
        let key = rule.key.to_string();
        rig.gateway().add_live_rule(&name, rule);
        assert!(rule_keys(&rig, &rig.handle(1).await?).await?.contains(&key));

        ready(retry(&rig, &rig.config, &rig.loaded, rig.request(1)?).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        assert!(rule_keys(&rig, &rig.handle(1).await?).await?.contains(&key));
        Ok(())
    })
    .await
}

struct Second {
    config: Config,
    repo: RepoFixture,
    _role: RoleFixture,
    loaded: LoadedRole,
}

async fn second(rig: &Rig) -> Result<Second, Box<dyn Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("src/lib.rs", b"\n").await?;
    repo.commit().await?;
    let role = RoleFixture::new("second")?;
    role.commit().await?;
    let loaded = rig
        .world
        .roles
        .load_path(role.root(), &CancellationToken::new())
        .await?;
    let config = Config::parse(&format!(
        "{}\n[workspaces.elsewhere]\npath = \"{}\"\n\n[roles.second]\nsource = {{ path = \"{}\" }}\n",
        rig.config.render(),
        repo.root().as_path().display(),
        role.root().as_path().display(),
    ))?;
    Ok(Second {
        config,
        repo,
        _role: role,
        loaded,
    })
}

#[tokio::test]
async fn a_reused_launch_id_under_another_workspace_is_refused_and_changes_nothing() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let kept = snapshot(&rig).await?;
        let other = second(&rig).await?;
        let request = request_for(&rig, other.repo.project().await?, "sentinel")?;
        let shown = refused(
            &rig,
            retry(&rig, &other.config, &rig.loaded, request).await?,
        )
        .await?;
        assert!(shown.contains("jackin.dev/workspace"), "{shown}");
        assert!(shown.contains("elsewhere"), "{shown}");
        assert_eq!(snapshot(&rig).await?, kept);

        ready(retry(&rig, &other.config, &rig.loaded, rig.request(1)?).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_reused_launch_id_under_another_role_with_the_same_image_id_is_refused() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let kept = snapshot(&rig).await?;
        let old = rig.final_image()?;
        let other = second(&rig).await?;
        let request = request_for(&rig, rig.project.clone(), "second")?;
        let plan = plan_launch(&other.config, &other.loaded, request)?;
        let (events, _receiver) = tokio::sync::mpsc::channel(256);
        let built = ensure_image(
            rig.world.engine.as_ref(),
            plan.image_request(),
            &events,
            &CancellationToken::new(),
        )
        .await?;
        rig.world.engine.pin_id(&built.tag, old.id);

        let request = request_for(&rig, rig.project.clone(), "second")?;
        let shown = refused(
            &rig,
            retry(&rig, &other.config, &other.loaded, request).await?,
        )
        .await?;
        assert!(shown.contains("jackin.dev/role"), "{shown}");
        assert!(shown.contains("second"), "{shown}");
        assert_eq!(snapshot(&rig).await?, kept);

        ready(retry(&rig, &other.config, &rig.loaded, rig.request(1)?).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}
