// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::error::Error;
use std::sync::Arc;

use jackin_core::{
    AccountName, Agent, ChatgptAccountId, EnvName, ImageId, ImageTag, LabelSelector, Labels,
    LaunchId, Phase, ProfileId, ProviderName, SandboxLabels, SandboxName, Secret,
};
use jackin_openshell::{
    Gateway, GatewayError, GatewayOp, Missing, PolicyDocument, ProfileSync, ProviderMaterial,
    ProviderProfile, ProviderSpec, ProviderState, Provisioning, SandboxQuery,
};
use jackin_testkit::{FakeEngine, FakeGateway, Fault};

use super::support::{IMAGE, TestResult, fake, guard, observe, sh, spec_with};

const ACCESS: &str = "Zq#7X!v9@Lm$2H%k4";
const REFRESH: &str = "Rt&5B$n8%Qw#3J!d6";

const fn every_window_has_a_symbol(secret: &str) -> bool {
    let bytes = secret.as_bytes();
    let mut start = 0;
    while start + 3 <= bytes.len() {
        if bytes[start].is_ascii_alphanumeric()
            && bytes[start + 1].is_ascii_alphanumeric()
            && bytes[start + 2].is_ascii_alphanumeric()
        {
            return false;
        }
        start += 1;
    }
    true
}

const _: () = assert!(every_window_has_a_symbol(ACCESS) && every_window_has_a_symbol(REFRESH));

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

pub(crate) fn login(
    profile: &ProfileId,
    name: &ProviderName,
) -> Result<ProviderSpec, Box<dyn Error>> {
    Ok(ProviderSpec {
        name: name.clone(),
        profile: profile.clone(),
        keys: Agent::Codex.spec().codex.openshell.keys,
        material: ProviderMaterial::OAuth2RefreshToken {
            access_token: Secret::new(ACCESS.to_owned()),
            refresh_token: Secret::new(REFRESH.to_owned()),
            client_id: "client".to_owned(),
            scopes: Vec::new(),
            expires_at: None,
            additional_credentials: std::collections::BTreeMap::from([(
                Agent::Codex
                    .spec()
                    .codex
                    .openshell
                    .keys
                    .account_id
                    .to_owned(),
                ChatgptAccountId::parse("0a1b2c3d-4e5f-6071-8293-a4b5c6d7e8f9")?.to_string(),
            )]),
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
        let profile = ProviderProfile::parse(&Agent::Codex.spec().codex.openshell.chatgpt_profile)?;
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

        let refused = fake.delete_provider(&provider).await;
        assert!(
            matches!(refused, Err(GatewayError::FailedPrecondition { .. })),
            "{refused:?}"
        );
        assert_eq!(fake.providers(), vec![provider.clone()]);
        let handle = fake.sandboxes();
        fake.delete_sandbox(spec.name()).await?;
        fake.delete_provider(&provider).await?;
        assert_eq!(fake.provider_state(&provider).await?, ProviderState::Absent);
        fake.delete_provider(&provider).await?;
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

#[tokio::test]
async fn a_provider_fault_is_struck_before_the_attached_check_and_clears() -> TestResult {
    guard(async {
        let fake = fake()?;
        let profile = ProviderProfile::parse(&Agent::Codex.spec().codex.openshell.chatgpt_profile)?;
        let attached = ProviderName::for_account(&AccountName::parse("work")?, Agent::Codex);
        fake.ensure_profile(&profile).await?;
        fake.put_provider(&login(&profile.id, &attached)?).await?;
        let spec = spec_with(
            &LaunchId::generate()?,
            IMAGE,
            vec![attached.clone()],
            sh("sleep 3600")?,
        )?;
        fake.create_sandbox(&spec).await?;

        fake.inject(Fault::FailNext {
            op: GatewayOp::Provider,
            error: GatewayError::Protocol {
                message: "injected".to_owned(),
            },
        });
        let struck = fake.delete_provider(&attached).await;
        assert!(
            matches!(&struck, Err(GatewayError::Protocol { message }) if message == "injected"),
            "{struck:?}"
        );
        assert!(fake.providers().contains(&attached));

        let refused = fake.delete_provider(&attached).await;
        assert!(
            matches!(refused, Err(GatewayError::FailedPrecondition { .. })),
            "{refused:?}"
        );
        assert!(fake.providers().contains(&attached));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn delete_provider_is_refused_while_a_sandbox_lists_it_and_ok_after() -> TestResult {
    guard(async {
        let fake = fake()?;
        let profile = ProviderProfile::parse(&Agent::Codex.spec().codex.openshell.chatgpt_profile)?;
        let attached = ProviderName::for_account(&AccountName::parse("work")?, Agent::Codex);
        let free = ProviderName::for_account(&AccountName::parse("other")?, Agent::Codex);
        fake.ensure_profile(&profile).await?;
        fake.put_provider(&login(&profile.id, &attached)?).await?;
        fake.put_provider(&login(&profile.id, &free)?).await?;
        let spec = spec_with(
            &LaunchId::generate()?,
            IMAGE,
            vec![attached.clone()],
            sh("sleep 3600")?,
        )?;
        fake.create_sandbox(&spec).await?;

        let refused = fake.delete_provider(&attached).await;
        let Err(GatewayError::FailedPrecondition { message }) = refused else {
            return Err(format!("{refused:?}").into());
        };
        assert!(message.contains(attached.as_str()), "{message}");
        assert!(message.contains(spec.name().as_str()), "{message}");
        assert!(fake.providers().contains(&attached));
        assert_eq!(
            fake.provider_state(&attached).await?,
            ProviderState::Present
        );

        fake.delete_provider(&free).await?;
        assert_eq!(fake.providers(), vec![attached.clone()]);

        fake.delete_sandbox(spec.name()).await?;
        fake.delete_provider(&attached).await?;
        fake.delete_provider(&attached).await?;
        assert_eq!(fake.provider_state(&attached).await?, ProviderState::Absent);
        assert!(fake.providers().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn inserting_an_unmanaged_sandbox_over_an_existing_name_is_already_exists_and_changes_nothing()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let first = SandboxName::parse("jackin-foreign-0001")?;
        let other = SandboxName::parse("jackin-foreign-0002")?;
        let original = Labels::from_pairs([("origin".to_owned(), "first".to_owned())])?;
        let replacement = Labels::from_pairs([("origin".to_owned(), "second".to_owned())])?;
        fake.insert_unmanaged(&first, original.clone(), Phase::Ready)?;

        let again = fake.insert_unmanaged(&first, replacement.clone(), Phase::Stopped);
        assert!(
            matches!(
                again,
                Err(GatewayError::AlreadyExists { what: Missing::Sandbox(ref name) }) if *name == first
            ),
            "{again:?}"
        );
        let shown = observe(&fake, &first).await?;
        assert_eq!((shown.labels, shown.phase), (original, Phase::Ready));

        let created = spec_with(&LaunchId::generate()?, IMAGE, Vec::new(), sh("sleep 3600")?)?;
        fake.create_sandbox(&created).await?;
        let over = fake.insert_unmanaged(created.name(), replacement, Phase::Stopped);
        assert!(
            matches!(
                over,
                Err(GatewayError::AlreadyExists { what: Missing::Sandbox(ref name) }) if name == created.name()
            ),
            "{over:?}"
        );
        let kept = observe(&fake, created.name()).await?;
        assert_eq!(kept.labels, created.labels().to_labels());

        fake.insert_unmanaged(&other, Labels::from_pairs([])?, Phase::Ready)?;
        assert_eq!(fake.sandboxes().len(), 3);
        Ok(())
    })
    .await
}

fn image_id(digit: char) -> Result<ImageId, Box<dyn Error>> {
    Ok(ImageId::parse(&format!(
        "sha256:{}",
        digit.to_string().repeat(64)
    ))?)
}

#[tokio::test]
async fn a_sandbox_keeps_the_image_id_its_tag_resolved_to_when_it_was_created() -> TestResult {
    guard(async {
        let fake = fake()?;
        let engine = Arc::new(FakeEngine::new());
        fake.link_engine(Arc::clone(&engine));
        let tag = ImageTag::parse("jackin.invalid/rebuilt:0123456789abcdef")?;
        let (old, new) = (image_id('a')?, image_id('b')?);
        engine.pin_id(&tag, old.clone());

        let first = spec_with(
            &LaunchId::generate()?,
            tag.as_str(),
            Vec::new(),
            sh("sleep 3600")?,
        )?;
        fake.create_sandbox(&first).await?;
        let handle = fake
            .sandbox(first.name())
            .ok_or("the sandbox is not listed")?;
        assert_eq!(handle.image_id(), Some(old.clone()));

        engine.pin_id(&tag, new.clone());
        assert_eq!(
            handle.image_id(),
            Some(old),
            "a retag moved a created sandbox"
        );

        let second = spec_with(
            &LaunchId::generate()?,
            tag.as_str(),
            Vec::new(),
            sh("sleep 3600")?,
        )?;
        fake.create_sandbox(&second).await?;
        let later = fake
            .sandbox(second.name())
            .ok_or("the sandbox is not listed")?;
        assert_eq!(later.image_id(), Some(new));

        let known = spec_with(&LaunchId::generate()?, IMAGE, Vec::new(), sh("sleep 3600")?)?;
        fake.create_sandbox(&known).await?;
        let unresolved = fake
            .sandbox(known.name())
            .ok_or("the sandbox is not listed")?;
        assert_eq!(
            unresolved.image_id(),
            None,
            "an image the engine never built"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn replacing_the_live_policy_of_an_unknown_sandbox_fails_loudly() -> TestResult {
    guard(async {
        let fake = fake()?;
        let policy = PolicyDocument::parse(
            "version: 1\nfilesystem_policy:\n  read_only:\n    - /bin\nlandlock:\n  compatibility: hard_requirement\nnetwork_policies: {}\n",
        )?;
        let unknown = SandboxName::parse("jackin-nobody-0001")?;
        match fake.replace_live_policy(&unknown, policy.clone()) {
            Err(GatewayError::NotFound {
                what: Missing::Sandbox(found),
            }) => assert_eq!(found, unknown),
            other => return Err(format!("expected NotFound, got {other:?}").into()),
        }

        let spec = spec_with(&LaunchId::generate()?, IMAGE, Vec::new(), sh("sleep 3600")?)?;
        fake.create_sandbox(&spec).await?;
        fake.replace_live_policy(spec.name(), policy)?;
        Ok(())
    })
    .await
}
