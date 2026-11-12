// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jackin_core::{
    Account, AccountName, Agent, ConfigEdit, ConfigError, CredentialSource, EnvName, HostPath,
    OpReference, Provider, ProviderName,
};
use jackin_session::{
    AccountError, KimiRegion, NativeCredentialError, NativeRefreshUnsupportedReason,
    plan_account_edits_for_agent,
};
use jackin_testkit::World;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, guard, register};

const KIMI_ACCESS: &str = "FAKE-KIMI-ACCESS-native";
const KIMI_REFRESH: &str = "FAKE-KIMI-REFRESH-native";
const KIMI_CLIENT: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const GLOBAL_SLOT: &str = "kimi-code-env-0e4f99c69cc27850";
const MINIMAX_ACCESS: &str = "FAKE-MINIMAX-ACCESS-native";
const MINIMAX_REFRESH: &str = "FAKE-MINIMAX-REFRESH-native";
const MIMO_KEY: &str = "FAKE-MIMO-KEY-native";

async fn write(path: &std::path::Path, contents: impl AsRef<[u8]>) -> TestResult {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, contents).await?;
    Ok(())
}

fn account(
    name: &str,
    provider: Provider,
    credential: CredentialSource,
) -> Result<Account, Box<dyn std::error::Error>> {
    Ok(Account::new(
        AccountName::parse(name)?,
        provider,
        credential,
    )?)
}

fn host_account(
    name: &str,
    provider: Provider,
    path: &HostPath,
) -> Result<Account, Box<dyn std::error::Error>> {
    account(
        name,
        provider,
        CredentialSource::HostLogin { path: path.clone() },
    )
}

fn kimi_config(region: KimiRegion) -> String {
    let (base, slot, oauth_host) = match region {
        KimiRegion::MainlandCn => ("https://api.kimi.com/coding/v1", "kimi-code", ""),
        KimiRegion::Global => (
            "https://api.kimi.ai/coding/v1",
            GLOBAL_SLOT,
            "oauth_host = \"https://auth.kimi.ai\"\n",
        ),
    };
    format!(
        "[providers.\"managed:kimi-code\"]\ntype = \"kimi\"\nbase_url = \"{base}\"\napi_key = \"\"\n\n[providers.\"managed:kimi-code\".oauth]\nstorage = \"file\"\nkey = \"oauth/{slot}\"\n{oauth_host}"
    )
}

async fn seed_kimi(
    root: &std::path::Path,
    region: KimiRegion,
    scope: &str,
) -> Result<HostPath, Box<dyn std::error::Error>> {
    write(&root.join("config.toml"), kimi_config(region)).await?;
    let slot = match region {
        KimiRegion::MainlandCn => "kimi-code",
        KimiRegion::Global => GLOBAL_SLOT,
    };
    write(
        &root.join("credentials").join(format!("{slot}.json")),
        json!({
            "access_token": KIMI_ACCESS,
            "refresh_token": KIMI_REFRESH,
            "expires_at": 1_800_000_000_u64,
            "expires_in": 3600,
            "scope": scope,
            "token_type": "Bearer"
        })
        .to_string(),
    )
    .await?;
    Ok(HostPath::parse(&std::fs::canonicalize(root)?)?)
}

async fn sync(
    world: &World,
    account: &Account,
    agent: Agent,
) -> Result<jackin_session::AccountSync, AccountError> {
    world
        .session
        .sync_account(account, agent, &CancellationToken::new())
        .await
}

#[tokio::test]
async fn kimi_native_logins_publish_regional_oauth_without_importing_scope() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let cn_dir = tempfile::tempdir()?;
        let global_dir = tempfile::tempdir()?;
        for (name, region, dir, profile) in [
            ("cn", KimiRegion::MainlandCn, &cn_dir, "kimi"),
            ("global", KimiRegion::Global, &global_dir, "kimi-global"),
        ] {
            let home = seed_kimi(dir.path(), region, "membership.read membership.write").await?;
            let account = host_account(name, Provider::Kimi, &home)?;
            register(&world, &account).await?;
            sync(&world, &account, Agent::KimiCode).await?;
            let provider = ProviderName::for_account(account.name(), Agent::KimiCode);
            let record = world.gateway.provider(&provider).ok_or("no provider")?;
            assert_eq!(record.profile.as_str(), profile);
            assert_eq!(record.access_key, "KIMI_API_KEY");
            assert_eq!(record.access_token, KIMI_ACCESS);
            assert_eq!(record.refresh_token, KIMI_REFRESH);
            assert_eq!(record.client_id, KIMI_CLIENT);
            assert!(record.scopes.is_empty());
            assert_eq!(record.expires_at, Some(1_800_000_000_000));
            assert!(record.additional_credentials.is_empty());
        }
        assert_eq!(world.gateway.profiles().len(), 2);
        assert_eq!(world.gateway.providers().len(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn native_agents_reject_every_static_source_before_gateway_effects() -> TestResult {
    guard(async {
        for (agent, provider) in [
            (Agent::MinimaxCode, Provider::Minimax),
            (Agent::MimoCode, Provider::Mimo),
            (Agent::KimiCode, Provider::Kimi),
        ] {
            for credential in [
                CredentialSource::Env {
                    var: EnvName::parse("JACKIN_NATIVE_TEST_KEY")?,
                },
                CredentialSource::OnePassword {
                    reference: OpReference::parse("op://Dev/Native/credential")?,
                    account: None,
                },
                CredentialSource::File {
                    path: HostPath::parse(std::path::Path::new("/tmp/native-static"))?,
                },
                CredentialSource::OmpHost,
            ] {
                let world = World::builder().build()?;
                let account = account("native", provider, credential)?;
                register(&world, &account).await?;
                let result = sync(&world, &account, agent).await;
                assert!(matches!(
                    result,
                    Err(AccountError::Config(ConfigError::CredentialNotEligible { agent: found, .. }))
                        if found == agent
                ));
                assert!(world.gateway.profiles().is_empty());
                assert!(world.gateway.providers().is_empty());
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn native_logins_cannot_serve_static_agent_tuples() -> TestResult {
    guard(async {
        for (agent, provider) in [
            (Agent::Omp, Provider::Kimi),
            (Agent::ClaudeCode, Provider::Kimi),
            (Agent::Codex, Provider::Kimi),
            (Agent::Omp, Provider::Minimax),
            (Agent::ClaudeCode, Provider::Minimax),
            (Agent::Codex, Provider::Minimax),
            (Agent::Omp, Provider::Mimo),
            (Agent::ClaudeCode, Provider::Mimo),
            (Agent::Codex, Provider::Mimo),
        ] {
            let world = World::builder().build()?;
            let path = HostPath::parse(std::path::Path::new("/tmp/native-login"))?;
            let account = host_account("native", provider, &path)?;
            register(&world, &account).await?;
            let result = sync(&world, &account, agent).await;
            assert!(matches!(
                result,
                Err(AccountError::Config(ConfigError::CredentialNotEligible { agent: found, .. }))
                    if found == agent
            ));
            assert!(world.gateway.profiles().is_empty());
            assert!(world.gateway.providers().is_empty());
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn invalid_native_source_change_preserves_present_provider_and_config() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        let home = seed_kimi(dir.path(), KimiRegion::MainlandCn, "membership.read").await?;
        let native = host_account("work", Provider::Kimi, &home)?;
        register(&world, &native).await?;
        sync(&world, &native, Agent::KimiCode).await?;
        let provider = ProviderName::for_account(native.name(), Agent::KimiCode);
        let before = world.gateway.provider(&provider).ok_or("no provider")?;
        let config = world.config.load().await?;
        let key = world.home().as_path().join("static-key");
        write(&key, "FAKE-STATIC-KIMI").await?;
        let credential = CredentialSource::File {
            path: HostPath::parse(&key)?,
        };
        let result = plan_account_edits_for_agent(
            &world.desk(),
            vec![ConfigEdit::SetAccountCredential {
                account: native.name().clone(),
                credential: credential.clone(),
            }],
            Agent::KimiCode,
            &CancellationToken::new(),
        )
        .await;
        assert!(result.is_err(), "the invalid source change was planned");
        assert_eq!(world.config.load().await?, config);
        assert_eq!(world.gateway.provider(&provider), Some(before.clone()));
        let changed = account("work", Provider::Kimi, credential)?;
        world
            .config
            .edit(vec![ConfigEdit::SetAccountCredential {
                account: native.name().clone(),
                credential: changed.credential().clone(),
            }])
            .await?;
        let result = sync(&world, &changed, Agent::KimiCode).await;
        assert!(
            result.is_err(),
            "the present provider hid an invalid source"
        );
        assert_eq!(world.gateway.provider(&provider), Some(before));
        Ok(())
    })
    .await
}

fn minimax_record_key(root: &HostPath) -> String {
    let mut hasher = Sha256::new();
    hasher.update(root.as_path().join("auth").to_string_lossy().as_bytes());
    hasher.update([0]);
    hasher.update(b"mcode-public");
    let account = URL_SAFE_NO_PAD.encode(hasher.finalize());
    format!("com.minimax.mcode.oauth.prod.cn\0{account}")
}

async fn seed_minimax(root: &std::path::Path) -> Result<HostPath, Box<dyn std::error::Error>> {
    let host = HostPath::parse(&std::fs::canonicalize(root)?)?;
    let key = minimax_record_key(&host);
    let credential = json!({
        "schemaVersion": 1,
        "accessToken": MINIMAX_ACCESS,
        "refreshToken": MINIMAX_REFRESH,
        "tokenType": "Bearer",
        "clientId": "mcode-public",
        "scopes": ["agent.default"],
        "audience": "agent-backend",
        "expiresAtMs": 1_800_000_000_123_u64,
        "generation": 7
    });
    let state = json!({
        "schemaVersion": 2,
        "status": "authenticated",
        "storeKind": "file",
        "clientId": "mcode-public",
        "scopes": ["agent.default"],
        "audience": "agent-backend",
        "buildEnv": "prod",
        "region": "cn",
        "expiresAtMs": 1_800_000_000_123_u64,
        "generation": 7
    });
    write(
        &root.join("auth/prod/cn/mcode-public/auth.json"),
        json!({"schemaVersion": 1, "records": {key: credential}}).to_string(),
    )
    .await?;
    write(
        &root.join("auth/prod/cn/mcode-public/auth-state.json"),
        state.to_string(),
    )
    .await?;
    Ok(host)
}

#[tokio::test]
async fn valid_minimax_native_material_is_audience_unsupported_before_effects() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        let home = seed_minimax(dir.path()).await?;
        let account = host_account("native", Provider::Minimax, &home)?;
        register(&world, &account).await?;
        let error = sync(&world, &account, Agent::MinimaxCode)
            .await
            .expect_err("MiniMax native refresh must stay unsupported");
        assert!(matches!(
            error,
            AccountError::NativeCredential(NativeCredentialError::NativeRefreshUnsupported(ref unsupported))
                if unsupported.reason == NativeRefreshUnsupportedReason::AudienceUnsupported
        ));
        assert!(world.gateway.profiles().is_empty());
        assert!(world.gateway.providers().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn valid_mimo_api_record_has_no_refresh_grant_before_effects() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        write(
            &dir.path().join("auth.json"),
            json!({"xiaomi": {"type": "api", "key": MIMO_KEY}}).to_string(),
        )
        .await?;
        let home = HostPath::parse(&std::fs::canonicalize(dir.path())?)?;
        let account = host_account("native", Provider::Mimo, &home)?;
        register(&world, &account).await?;
        let error = sync(&world, &account, Agent::MimoCode)
            .await
            .expect_err("MiMo native refresh must stay unsupported");
        assert!(matches!(
            error,
            AccountError::NativeCredential(NativeCredentialError::NativeRefreshUnsupported(ref unsupported))
                if unsupported.reason == NativeRefreshUnsupportedReason::NoRefreshGrant
        ));
        assert!(world.gateway.profiles().is_empty());
        assert!(world.gateway.providers().is_empty());
        Ok(())
    })
    .await
}
