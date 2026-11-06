// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, ChatgptAccountId, ProfileId, ProviderName, Secret};
use jackin_openshell::{
    Gateway, GatewayError, Missing, ProfileSync, ProviderMaterial, ProviderProfile, ProviderSpec,
    ProviderState,
};
use openshell_sdk::raw::proto::{ObjectMeta, Provider, ProviderCredentialRefreshStrategy};
use tonic::Code;

use super::harness::{WORKSPACE, connect, env, guard};
use super::server::{Fault, State};

const ACCESS: &str = "Zq7#Xv9!Lm2@Hk4wP";
const REFRESH: &str = "Rt5$Bn8%Qw3&Jd6yS";
const ACCOUNT_ID: &str = "0a1b2c3d-4e5f-6071-8293-a4b5c6d7e8f9";
const PROVIDER: &str = "jackin-work-codex";

fn profile() -> ProviderProfile {
    ProviderProfile::parse(&Agent::Codex.spec().codex.openshell.chatgpt_profile).unwrap()
}

fn spec(access: &str, refresh: &str) -> ProviderSpec {
    ProviderSpec {
        name: ProviderName::parse(PROVIDER).unwrap(),
        profile: ProfileId::parse("codex").unwrap(),
        keys: Agent::Codex.spec().codex.openshell.keys,
        material: ProviderMaterial::ChatgptLogin {
            access_token: Secret::new(access.to_owned()),
            refresh_token: Secret::new(refresh.to_owned()),
            client_id: "app_client".to_owned(),
            chatgpt_account_id: ChatgptAccountId::parse(ACCOUNT_ID).unwrap(),
        },
    }
}

fn stored_profile(state: &State) -> openshell_sdk::raw::proto::ProviderProfile {
    state.profiles[&(WORKSPACE.to_owned(), "codex".to_owned())].clone()
}

fn normalized(
    mut profile: openshell_sdk::raw::proto::ProviderProfile,
) -> openshell_sdk::raw::proto::ProviderProfile {
    profile.resource_version = 0;
    profile.source.clear();
    profile
}

fn assert_no_secret(text: &str) {
    for secret in [ACCESS, REFRESH] {
        for window in secret.as_bytes().windows(3) {
            let window = std::str::from_utf8(window).unwrap();
            assert!(!text.contains(window), "{window} leaked in {text}");
        }
    }
}

#[tokio::test]
async fn ensure_profile_imports_then_reports_unchanged_then_updates() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    let profile = profile();
    assert_eq!(
        guard(gateway.ensure_profile(&profile)).await.unwrap(),
        ProfileSync::Imported
    );
    let imported = stored_profile(&env.server.state());
    assert_eq!(imported.id, "codex");
    assert_eq!(imported.endpoints.len(), 4);
    assert!(!imported.credentials.is_empty());
    assert_eq!(
        guard(gateway.ensure_profile(&profile)).await.unwrap(),
        ProfileSync::Unchanged
    );
    {
        let state = env.server.state();
        assert_eq!(
            stored_profile(&state).resource_version,
            imported.resource_version
        );
        assert_eq!(state.profile_updates, 0);
    }
    {
        let mut state = env.server.state();
        let stored = state
            .profiles
            .get_mut(&(WORKSPACE.to_owned(), "codex".to_owned()))
            .unwrap();
        "edited".clone_into(&mut stored.description);
        stored.resource_version += 1;
    }
    assert_eq!(
        guard(gateway.ensure_profile(&profile)).await.unwrap(),
        ProfileSync::Updated
    );
    let state = env.server.state();
    let updated = stored_profile(&state);
    assert_eq!(normalized(updated.clone()), normalized(imported.clone()));
    assert!(updated.resource_version > imported.resource_version + 1);
}

#[tokio::test]
async fn ensure_profile_stale_version_keeps_the_concurrent_writers_content() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    let profile = profile();
    guard(gateway.ensure_profile(&profile)).await.unwrap();
    {
        let mut state = env.server.state();
        let key = (WORKSPACE.to_owned(), "codex".to_owned());
        "operator".clone_into(&mut state.profiles.get_mut(&key).unwrap().description);
        state.after_get_profile = Some(Box::new(|state: &mut State| {
            let stored = state
                .profiles
                .get_mut(&(WORKSPACE.to_owned(), "codex".to_owned()))
                .unwrap();
            "concurrent".clone_into(&mut stored.description);
            stored.resource_version += 1;
        }));
    }
    let error = guard(gateway.ensure_profile(&profile)).await.unwrap_err();
    assert!(
        matches!(error, GatewayError::FailedPrecondition { .. }),
        "{error:?}"
    );
    assert_eq!(
        stored_profile(&env.server.state()).description,
        "concurrent"
    );
    assert_eq!(
        guard(gateway.ensure_profile(&profile)).await.unwrap(),
        ProfileSync::Updated
    );
    assert_ne!(
        stored_profile(&env.server.state()).description,
        "concurrent"
    );
}

#[tokio::test]
async fn put_provider_creates_updates_and_configures_refresh() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    guard(gateway.ensure_profile(&profile())).await.unwrap();
    let keys = Agent::Codex.spec().codex.openshell.keys;
    let provider_key = (WORKSPACE.to_owned(), PROVIDER.to_owned());
    let refresh_key = (
        WORKSPACE.to_owned(),
        PROVIDER.to_owned(),
        keys.access_token.to_owned(),
    );

    guard(gateway.put_provider(&spec(ACCESS, REFRESH)))
        .await
        .unwrap();
    {
        let state = env.server.state();
        assert_eq!(state.providers.len(), 1);
        let provider = &state.providers[&provider_key];
        assert_eq!(provider.r#type, "codex");
        assert_eq!(provider.credentials[keys.access_token], ACCESS);
        assert_eq!(provider.credentials[keys.account_id], ACCOUNT_ID);
        assert!(!provider.credentials.values().any(|value| value == REFRESH));
        let refresh = &state.refresh[&refresh_key];
        assert_eq!(
            refresh.strategy,
            ProviderCredentialRefreshStrategy::Oauth2RefreshToken as i32
        );
        assert_eq!(refresh.material["refresh_token"], REFRESH);
        assert_eq!(refresh.material["client_id"], "app_client");
        assert!(!refresh.material.contains_key("token_url"));
        assert_eq!(refresh.secret_keys, vec!["refresh_token".to_owned()]);
    }

    let fresh_access = "new-access-token-value";
    guard(gateway.put_provider(&spec(fresh_access, "new-refresh-token")))
        .await
        .unwrap();
    {
        let state = env.server.state();
        assert_eq!(state.providers.len(), 1);
        assert_eq!(
            state.providers[&provider_key].credentials[keys.access_token],
            fresh_access
        );
        assert_eq!(state.refresh.len(), 1);
        assert_eq!(
            state.refresh[&refresh_key].material["refresh_token"],
            "new-refresh-token"
        );
    }
    guard(gateway.put_provider(&spec(fresh_access, "new-refresh-token")))
        .await
        .unwrap();
    assert_eq!(env.server.state().providers.len(), 1);
    assert_eq!(env.server.state().refresh.len(), 1);

    env.server.state().faults.insert(
        "configure_provider_refresh",
        Fault::Status(
            Code::Internal,
            format!("refresh failed for {ACCESS} and {REFRESH}"),
        ),
    );
    let error = guard(gateway.put_provider(&spec(ACCESS, REFRESH)))
        .await
        .unwrap_err();
    assert_no_secret(&error.to_string());
    assert_no_secret(&format!("{error:?}"));
    env.server.state().faults.clear();
    guard(gateway.put_provider(&spec(ACCESS, REFRESH)))
        .await
        .unwrap();
    let state = env.server.state();
    assert_eq!(
        state.providers[&provider_key].credentials[keys.access_token],
        ACCESS
    );
    assert_eq!(
        state.refresh[&refresh_key].material["refresh_token"],
        REFRESH
    );
}

#[tokio::test]
async fn provider_state_delete_and_missing_profile() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    let name = ProviderName::parse(PROVIDER).unwrap();
    assert_eq!(
        guard(gateway.provider_state(&name)).await.unwrap(),
        ProviderState::Absent
    );

    let error = guard(gateway.put_provider(&spec(ACCESS, REFRESH)))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        GatewayError::NotFound { what: Missing::Profile(ref id) } if id.as_str() == "codex"
    ));
    assert!(env.server.state().providers.is_empty());

    guard(gateway.ensure_profile(&profile())).await.unwrap();
    guard(gateway.put_provider(&spec(ACCESS, REFRESH)))
        .await
        .unwrap();
    assert_eq!(
        guard(gateway.provider_state(&name)).await.unwrap(),
        ProviderState::Present
    );
    guard(gateway.delete_provider(&name)).await.unwrap();
    assert_eq!(
        guard(gateway.provider_state(&name)).await.unwrap(),
        ProviderState::Absent
    );
    assert!(env.server.state().providers.is_empty());
    guard(gateway.delete_provider(&name)).await.unwrap();
}

fn codex_key(workspace: &str) -> (String, String) {
    (workspace.to_owned(), "codex".to_owned())
}

#[tokio::test]
async fn delete_profile_removes_only_the_configured_workspaces_profile_and_is_idempotent() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    let other = connect(&env, "default").await;
    let id = ProfileId::parse("codex").unwrap();
    guard(gateway.ensure_profile(&profile())).await.unwrap();
    guard(other.ensure_profile(&profile())).await.unwrap();

    guard(gateway.delete_profile(&id)).await.unwrap();
    {
        let state = env.server.state();
        assert!(!state.profiles.contains_key(&codex_key(WORKSPACE)));
        assert!(state.profiles.contains_key(&codex_key("default")));
    }
    guard(gateway.delete_profile(&id)).await.unwrap();
    assert!(
        env.server
            .state()
            .profiles
            .contains_key(&codex_key("default"))
    );

    guard(gateway.ensure_profile(&profile())).await.unwrap();
    guard(gateway.put_provider(&spec(ACCESS, REFRESH)))
        .await
        .unwrap();
    let error = guard(gateway.delete_profile(&id)).await.unwrap_err();
    assert!(
        matches!(error, GatewayError::FailedPrecondition { ref message } if message.contains("in use")),
        "{error:?}"
    );
    assert!(
        env.server
            .state()
            .profiles
            .contains_key(&codex_key(WORKSPACE))
    );
    guard(gateway.delete_provider(&ProviderName::parse(PROVIDER).unwrap()))
        .await
        .unwrap();
    guard(gateway.delete_profile(&id)).await.unwrap();
    assert!(
        !env.server
            .state()
            .profiles
            .contains_key(&codex_key(WORKSPACE))
    );
}

#[tokio::test]
async fn ensure_profile_converges_when_a_concurrent_writer_imports_first() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    let profile = profile();
    guard(gateway.ensure_profile(&profile)).await.unwrap();
    let asset = stored_profile(&env.server.state());
    env.server.state().profiles.clear();

    let same = asset.clone();
    env.server.state().before_import = Some(Box::new(move |state: &mut State| {
        state.profiles.insert(codex_key(WORKSPACE), same.clone());
    }));
    assert_eq!(
        guard(gateway.ensure_profile(&profile)).await.unwrap(),
        ProfileSync::Unchanged
    );
    {
        let state = env.server.state();
        assert_eq!(state.profile_updates, 0);
        assert_eq!(
            stored_profile(&state).resource_version,
            asset.resource_version
        );
    }

    env.server.state().profiles.clear();
    let mut different = asset.clone();
    "operator".clone_into(&mut different.description);
    env.server.state().before_import = Some(Box::new(move |state: &mut State| {
        state
            .profiles
            .insert(codex_key(WORKSPACE), different.clone());
    }));
    assert_eq!(
        guard(gateway.ensure_profile(&profile)).await.unwrap(),
        ProfileSync::Updated
    );
    {
        let state = env.server.state();
        let stored = stored_profile(&state);
        assert_eq!(normalized(stored.clone()), normalized(asset.clone()));
        assert!(stored.resource_version > asset.resource_version);
        assert_eq!(state.profile_updates, 1);
    }

    {
        let mut state = env.server.state();
        state.profiles.clear();
        state.import_rejection = Some((
            "endpoints".to_owned(),
            "the endpoint list is invalid".to_owned(),
        ));
    }
    let error = guard(gateway.ensure_profile(&profile)).await.unwrap_err();
    assert!(
        matches!(&error, GatewayError::InvalidArgument { field, message }
            if field == "endpoints" && message.contains("invalid")),
        "{error:?}"
    );
    assert!(env.server.state().profiles.is_empty());
}

#[tokio::test]
async fn put_provider_converges_when_a_concurrent_writer_creates_first() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    guard(gateway.ensure_profile(&profile())).await.unwrap();
    let keys = Agent::Codex.spec().codex.openshell.keys;
    let provider_key = (WORKSPACE.to_owned(), PROVIDER.to_owned());
    let refresh_key = (
        WORKSPACE.to_owned(),
        PROVIDER.to_owned(),
        keys.access_token.to_owned(),
    );

    let rival_key = provider_key.clone();
    env.server.state().before_create_provider = Some(Box::new(move |state: &mut State| {
        state.providers.insert(
            rival_key.clone(),
            Provider {
                metadata: Some(ObjectMeta {
                    name: PROVIDER.to_owned(),
                    ..ObjectMeta::default()
                }),
                r#type: "codex".to_owned(),
                profile_workspace: WORKSPACE.to_owned(),
                credentials: [
                    (keys.access_token.to_owned(), "rival-access".to_owned()),
                    (keys.account_id.to_owned(), "rival-account".to_owned()),
                ]
                .into(),
                ..Provider::default()
            },
        );
    }));
    guard(gateway.put_provider(&spec("fresh-access-token", "fresh-refresh-token")))
        .await
        .unwrap();
    {
        let state = env.server.state();
        assert_eq!(state.providers.len(), 1);
        let provider = &state.providers[&provider_key];
        assert_eq!(
            provider.credentials[keys.access_token],
            "fresh-access-token"
        );
        assert_eq!(provider.credentials[keys.account_id], ACCOUNT_ID);
        assert_eq!(state.refresh.len(), 1);
        assert_eq!(
            state.refresh[&refresh_key].material["refresh_token"],
            "fresh-refresh-token"
        );
    }

    {
        let mut state = env.server.state();
        state.providers.clear();
        state.refresh.clear();
        state.faults.insert(
            "create_provider",
            Fault::Status(Code::AlreadyExists, "provider already exists".to_owned()),
        );
    }
    let error = guard(gateway.put_provider(&spec(ACCESS, REFRESH)))
        .await
        .unwrap_err();
    assert!(
        matches!(&error, GatewayError::AlreadyExists { what: Missing::Provider(name) } if name.as_str() == PROVIDER),
        "{error:?}"
    );
    assert!(env.server.state().providers.is_empty());
}
