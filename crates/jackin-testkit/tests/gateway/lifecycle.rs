// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::error::Error;

use jackin_core::{
    AccountName, Agent, ChatgptAccountId, EnvName, LabelSelector, LaunchId, Phase, ProfileId,
    ProviderName, SandboxLabels, Secret,
};
use jackin_openshell::{
    Gateway, GatewayError, Missing, ProfileSync, ProviderMaterial, ProviderProfile, ProviderSpec,
    ProviderState, Provisioning, SandboxQuery,
};
use jackin_testkit::FakeGateway;

use super::support::{IMAGE, TestResult, fake, guard, observe, sh, spec_with};

const ACCESS: &str = "Zq7#Xv9!Lm2@Hk4wP";
const REFRESH: &str = "Rt5$Bn8%Qw3&Jd6yS";

fn leaks(text: &str, secret: &str) -> bool {
    let chars: Vec<char> = secret.chars().collect();
    chars
        .windows(3)
        .any(|window| text.contains(&window.iter().collect::<String>()))
}

#[tokio::test]
async fn create_find_and_delete_round_trip_the_sandbox_state() -> TestResult {
    guard(async {
        let fake = fake()?;
        let launch = LaunchId::generate()?;
        let spec = spec_with(&launch, IMAGE, Vec::new(), sh("sleep 3600")?)?;
        let name = spec.name().clone();
        let created = fake.create_sandbox(&spec).await?;
        assert_eq!(created.phase, Phase::Provisioning);
        assert!(matches!(created.provisioning, Provisioning::Normal));
        let handles = fake.sandboxes();
        assert_eq!(handles.len(), 1);
        assert_eq!(handles[0].labels(), spec.labels().to_labels());
        assert_eq!(
            handles[0].environment(),
            [(EnvName::parse("JACKIN_LAUNCH_ID")?, launch.to_string())].into()
        );

        let by_launch = fake
            .find_sandboxes(&SandboxQuery::Labels(LabelSelector::launch(&launch)))
            .await?;
        assert_eq!(by_launch.len(), 1);
        assert_eq!(by_launch[0].name, name);
        assert_eq!(
            SandboxLabels::from_labels(&by_launch[0].labels)?,
            *spec.labels()
        );
        let managed = fake
            .find_sandboxes(&SandboxQuery::Labels(LabelSelector::managed()))
            .await?;
        assert_eq!(managed.len(), 1);
        let other = LaunchId::generate()?;
        let none = fake
            .find_sandboxes(&SandboxQuery::Labels(LabelSelector::launch(&other)))
            .await?;
        assert!(none.is_empty());
        let missing = spec_with(&other, IMAGE, Vec::new(), sh("sleep 3600")?)?;
        let none = fake
            .find_sandboxes(&SandboxQuery::Name(missing.name().clone()))
            .await?;
        assert!(none.is_empty());

        let duplicate = fake.create_sandbox(&spec).await;
        assert!(
            matches!(&duplicate, Err(GatewayError::AlreadyExists { what }) if *what == Missing::Sandbox(name.clone())),
            "{duplicate:?}"
        );
        assert_eq!(fake.sandboxes().len(), 1);
        assert_eq!(fake.sandboxes()[0].labels(), spec.labels().to_labels());

        fake.delete_sandbox(&name).await?;
        assert!(fake.sandboxes().is_empty());
        fake.delete_sandbox(&name).await?;
        Ok(())
    })
    .await
}

fn login(profile: &ProfileId, name: &ProviderName) -> Result<ProviderSpec, Box<dyn Error>> {
    Ok(ProviderSpec {
        name: name.clone(),
        profile: profile.clone(),
        keys: Agent::Codex.spec().openshell.keys,
        material: ProviderMaterial::ChatgptLogin {
            access_token: Secret::new(ACCESS.to_owned()),
            refresh_token: Secret::new(REFRESH.to_owned()),
            client_id: "client".to_owned(),
            chatgpt_account_id: ChatgptAccountId::parse("0a1b2c3d-4e5f-6071-8293-a4b5c6d7e8f9")?,
        },
    })
}

async fn profile_flow(
    fake: &FakeGateway,
    profile: &ProviderProfile,
    provider: &ProviderName,
) -> Result<String, Box<dyn Error>> {
    let early = fake.put_provider(&login(&profile.id, provider)?).await;
    let early = match early {
        Err(error @ GatewayError::NotFound { .. }) => error,
        other => return Err(format!("put without a profile: {other:?}").into()),
    };
    assert!(
        matches!(&early, GatewayError::NotFound { what } if *what == Missing::Profile(profile.id.clone()))
    );
    assert!(fake.providers().is_empty());
    assert_eq!(fake.ensure_profile(profile).await?, ProfileSync::Imported);
    assert_eq!(fake.ensure_profile(profile).await?, ProfileSync::Unchanged);
    let mut changed = profile.clone();
    changed.binaries.pop();
    assert_eq!(fake.ensure_profile(&changed).await?, ProfileSync::Updated);
    assert_eq!(fake.ensure_profile(&changed).await?, ProfileSync::Unchanged);
    assert_eq!(fake.ensure_profile(profile).await?, ProfileSync::Updated);
    assert_eq!(fake.profiles(), vec![profile.id.clone()]);
    Ok(format!("{early:?}"))
}

#[tokio::test]
async fn providers_profiles_and_detail_follow_the_gateway_contract() -> TestResult {
    guard(async {
        let fake = fake()?;
        let profile = ProviderProfile::parse(&Agent::Codex.spec().openshell.chatgpt_profile)?;
        let provider = ProviderName::for_account(&AccountName::parse("work")?, Agent::Codex);
        let early_error = profile_flow(&fake, &profile, &provider).await?;
        fake.put_provider(&login(&profile.id, &provider)?).await?;
        fake.put_provider(&login(&profile.id, &provider)?).await?;
        assert_eq!(fake.providers(), vec![provider.clone()]);
        assert_eq!(
            fake.provider_state(&provider).await?,
            ProviderState::Present
        );
        assert!(matches!(
            fake.delete_profile(&profile.id).await,
            Err(GatewayError::FailedPrecondition { .. })
        ));

        let spec = spec_with(
            &LaunchId::generate()?,
            IMAGE,
            vec![provider.clone()],
            sh("sleep 3600")?,
        )?;
        fake.create_sandbox(&spec).await?;
        let detail = fake.sandbox_detail(spec.name()).await?;
        assert_eq!(detail.providers, vec![provider.clone()]);
        assert!(detail.policy.loaded);
        assert!(detail.policy.version >= 1);
        let listed = observe(&fake, spec.name()).await?;
        assert_eq!(format!("{:?}", detail.view), format!("{listed:?}"));
        let keys: BTreeSet<String> = detail
            .policy
            .document
            .network_rules()
            .iter()
            .map(|rule| rule.key.as_str().to_owned())
            .collect();
        let expected: BTreeSet<String> = ["docker_engine", "_provider_jackin_work_codex"]
            .map(str::to_owned)
            .into();
        assert_eq!(keys, expected);

        fake.delete_provider(&provider).await?;
        assert_eq!(fake.provider_state(&provider).await?, ProviderState::Absent);
        fake.delete_provider(&provider).await?;
        let handle = fake.sandboxes();
        for secret in [ACCESS, REFRESH] {
            for text in [
                format!("{fake:?}"),
                format!("{handle:?}"),
                early_error.clone(),
            ] {
                assert!(!leaks(&text, secret), "{secret} leaked into {text}");
            }
        }
        Ok(())
    })
    .await
}

fn pending_once<F: Future>(call: F) -> bool {
    let mut call = std::pin::pin!(call);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    call.as_mut().poll(&mut cx).is_pending()
}

#[tokio::test]
async fn every_gateway_call_yields_to_the_scheduler_before_it_acts() -> TestResult {
    guard(async {
        let fake = fake()?;
        let sandbox = spec_with(&LaunchId::generate()?, IMAGE, Vec::new(), sh("sleep 3600")?)?;
        let selector =
            SandboxQuery::Labels(LabelSelector::launch(&sandbox.labels().planned.launch));
        assert!(pending_once(fake.find_sandboxes(&selector)));
        assert!(pending_once(fake.create_sandbox(&sandbox)));
        assert!(fake.sandboxes().is_empty());
        fake.create_sandbox(&sandbox).await?;
        let name = observe(&fake, sandbox.name()).await?.name;
        assert!(pending_once(fake.read_files(&name, &[])));
        assert!(pending_once(
            fake.provider_state(&ProviderName::parse("p")?)
        ));
        Ok(())
    })
    .await
}
