// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, Agent, CredentialSource, HostPath, Provider, ProviderName,
};
use jackin_session::{AccountError, grok_auth_file, read_grok_login, read_muse_host_metadata};
use jackin_testkit::World;
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, guard, register};

const GROK_A: &str = "FAKE-GROK-A-111";
const GROK_B: &str = "FAKE-GROK-B-222";
const ISSUER: &str = "https://auth.x.ai";

fn host_login_account(
    name: &str,
    provider: Provider,
    path: &HostPath,
) -> Result<Account, Box<dyn std::error::Error>> {
    Ok(Account::new(
        AccountName::parse(name)?,
        provider,
        CredentialSource::HostLogin { path: path.clone() },
    )?)
}

async fn write_file(path: &std::path::Path, text: &str) -> TestResult {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, text).await?;
    Ok(())
}

#[tokio::test]
async fn grok_reader_taxonomy_is_typed_and_secret_free() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        let base = dir.path().join("auth.json");
        let hp = HostPath::parse(&base)?;
        let missing = read_grok_login(world.fs.as_ref(), &hp).await;
        assert!(missing.is_err());
        write_file(&base, "{not json").await?;
        let malformed = read_grok_login(world.fs.as_ref(), &hp).await.unwrap_err();
        write_file(
            &base,
            &serde_json::json!({ISSUER: {"other": GROK_A}}).to_string(),
        )
        .await?;
        let no_key = read_grok_login(world.fs.as_ref(), &hp).await.unwrap_err();
        let dirpath = dir.path().join("dir.json");
        tokio::fs::create_dir(&dirpath).await?;
        let not_regular = read_grok_login(world.fs.as_ref(), &HostPath::parse(&dirpath)?)
            .await
            .unwrap_err();
        write_file(&base, &"x".repeat(1024 * 1024 + 1)).await?;
        let oversized = read_grok_login(world.fs.as_ref(), &hp).await.unwrap_err();
        for error in [&malformed, &no_key, &not_regular, &oversized] {
            let text = format!("{error} {error:?}");
            assert!(!text.contains(GROK_A), "{text}");
        }
        assert_ne!(format!("{malformed:?}"), format!("{no_key:?}"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn grok_sync_creates_t03_provider_with_key_only_in_record() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("auth.json");
        write_file(
            &file,
            &serde_json::json!({ISSUER: {"key": GROK_A}}).to_string(),
        )
        .await?;
        let parsed = jackin_core::Config::parse(&format!(
            "[accounts.alpha]\nprovider = \"grok\"\ncredential = {{ source = \"host-login\", path = \"{}\" }}\n",
            file.display()
        ))?;
        assert!(parsed.render().contains("source = \"host-login\""));
        let account = host_login_account("alpha", Provider::Grok, &HostPath::parse(&file)?)?;
        register(&world, &account).await?;
        let login = read_grok_login(world.fs.as_ref(), &HostPath::parse(&file)?).await?;
        assert_eq!(login.issuer, ISSUER);
        assert_eq!(login.session_key.expose(), GROK_A);
        let synced = world
            .session
            .sync_account(&account, Agent::GrokBuild, &CancellationToken::new())
            .await?;
        let name = ProviderName::for_account(&AccountName::parse("alpha")?, Agent::GrokBuild);
        assert_eq!(name.as_str(), "jackin-alpha-grok");
        assert!(matches!(synced, jackin_session::AccountSync::ProviderReady { ref provider, .. } if *provider == name));
        let record = world.gateway.provider(&name).ok_or("no provider")?;
        assert_eq!(record.profile.as_str(), "grok");
        assert_eq!(record.access_token, GROK_A);
        assert_eq!(record.access_key, "access_token");
        assert!(!format!("{synced:?}").contains(GROK_A));
        let planned = grok_auth_file(ISSUER);
        assert_eq!(planned.mode.0, 0o600);
        let parsed: serde_json::Value = serde_json::from_str(&planned.text)?;
        assert_eq!(parsed, serde_json::json!({ISSUER: {"key": "placeholder"}}));
        assert!(!planned.text.contains(GROK_A));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn muse_metadata_reader_and_metadata_only_sync() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("auth.json");
        let hp = HostPath::parse(&file)?;
        assert!(read_muse_host_metadata(world.fs.as_ref(), &hp).await.is_err());
        write_file(
            &file,
            &serde_json::json!({"providers": {"meta": {"mechanism": "oauth", "storage": "os-store", "subs_active": true}}})
                .to_string(),
        )
        .await?;
        let meta = read_muse_host_metadata(world.fs.as_ref(), &hp).await?;
        assert!(meta.subs_active);
        assert_eq!(meta.storage, "os-store");
        write_file(
            &file,
            &serde_json::json!({"providers": {"meta": {"mechanism": "oauth", "storage": "os-store", "subs_active": false}}})
                .to_string(),
        )
        .await?;
        assert!(!read_muse_host_metadata(world.fs.as_ref(), &hp).await?.subs_active);
        let account = host_login_account("museone", Provider::Muse, &hp)?;
        register(&world, &account).await?;
        let result = world
            .session
            .sync_account(&account, Agent::MuseCode, &CancellationToken::new())
            .await;
        let text = result.as_ref().err().map(ToString::to_string).unwrap_or_default();
        assert!(matches!(result, Err(AccountError::HostLoginStoreUnsupported { .. })), "{text}");
        assert!(text.contains("not yet supported"), "{text}");
        assert!(world.gateway.providers().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn grok_accounts_are_isolated_in_providers_and_launch_plan_names() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        let mut accounts = Vec::new();
        for (name, token) in [("alpha", GROK_A), ("beta", GROK_B)] {
            let file = dir.path().join(format!("{name}.json"));
            write_file(
                &file,
                &serde_json::json!({ISSUER: {"key": token}}).to_string(),
            )
            .await?;
            let account = host_login_account(name, Provider::Grok, &HostPath::parse(&file)?)?;
            register(&world, &account).await?;
            world
                .session
                .sync_account(&account, Agent::GrokBuild, &CancellationToken::new())
                .await?;
            accounts.push(account);
        }
        let a = ProviderName::for_account(&AccountName::parse("alpha")?, Agent::GrokBuild);
        let b = ProviderName::for_account(&AccountName::parse("beta")?, Agent::GrokBuild);
        assert_ne!(a, b);
        assert_eq!(
            world.gateway.provider(&a).ok_or("no a")?.access_token,
            GROK_A
        );
        assert_eq!(
            world.gateway.provider(&b).ok_or("no b")?.access_token,
            GROK_B
        );
        let launch_providers = [ProviderName::for_account(
            &AccountName::parse("beta")?,
            Agent::GrokBuild,
        )];
        assert_eq!(launch_providers, [b.clone()]);
        assert!(!format!("{launch_providers:?}").contains(GROK_A));
        assert!(!format!("{launch_providers:?}").contains("alpha"));
        Ok(())
    })
    .await
}
