// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, Agent, ConfigEdit, CredentialSource, HostPath, Program, Provider,
    ProviderName,
};
use jackin_session::{
    AccountError, grok_auth_file, plan_account_edits_for_agent, read_grok_login,
    read_muse_host_metadata,
};
use jackin_testkit::{ProcessReply, ScriptedProgram, World};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, guard, register};

const GROK_A: &str = "FAKE-GROK-A-111";
const GROK_B: &str = "FAKE-GROK-B-222";
const ISSUER: &str = "https://auth.x.ai";
const MUSE_API: &str = "FAKE-MUSE-API-333";
const MUSE_ACCESS: &str = "FAKE-MUSE-ACCESS-444";
const SECURITY_ARGS: [&str; 6] = [
    "find-generic-password",
    "-s",
    "ai.meta.dev.credentials",
    "-a",
    "meta",
    "-w",
];

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

fn profile_env_vars(
    agent: Agent,
    provider: Provider,
) -> Result<std::collections::BTreeSet<String>, Box<dyn std::error::Error>> {
    let tuple = jackin_core::lookup_tuple(agent, provider)?;
    let asset = jackin_core::tuple_profile_asset(tuple);
    Ok(asset
        .yaml
        .lines()
        .filter_map(|line| line.trim().strip_prefix("env_vars: ["))
        .filter_map(|rest| rest.strip_suffix(']'))
        .flat_map(|list| list.split(',').map(|name| name.trim().to_owned()))
        .collect())
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
        assert_eq!(record.access_key, "XAI_API_KEY");
        assert!(
            profile_env_vars(Agent::GrokBuild, Provider::Grok)?.contains(&record.access_key),
            "{}",
            record.access_key
        );
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
async fn muse_metadata_reader() -> TestResult {
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
        Ok(())
    })
    .await
}

async fn muse_world(
    storage: &str,
    subs_active: bool,
) -> Result<(World, Account, tempfile::TempDir), Box<dyn std::error::Error>> {
    let world = World::builder().build()?;
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("auth.json");
    write_file(
        &file,
        &serde_json::json!({"providers": {"meta": {"mechanism": "oauth", "storage": storage, "subs_active": subs_active}}})
            .to_string(),
    )
    .await?;
    let account = host_login_account("museone", Provider::Muse, &HostPath::parse(&file)?)?;
    register(&world, &account).await?;
    Ok((world, account, dir))
}

fn security_seen(world: &World) -> Vec<jackin_testkit::SeenCall> {
    world
        .runner
        .scripted()
        .seen()
        .into_iter()
        .filter(|call| call.program == Program::Security)
        .collect()
}

#[tokio::test]
async fn muse_keychain_sync_creates_provider_with_access_token_only() -> TestResult {
    guard(async {
        let (world, account, _dir) = muse_world("keychain", true).await?;
        world.runner.scripted().expect(
            ScriptedProgram::Security,
            &SECURITY_ARGS,
            ProcessReply::exited(
                0,
                serde_json::json!({"api_key": MUSE_API, "access_token": MUSE_ACCESS})
                    .to_string()
                    .as_bytes(),
                b"",
            ),
        );
        let synced = world
            .session
            .sync_account(&account, Agent::MuseCode, &CancellationToken::new())
            .await?;
        let name = ProviderName::for_account(&AccountName::parse("museone")?, Agent::MuseCode);
        assert_eq!(name.as_str(), "jackin-museone-muse");
        assert!(matches!(synced, jackin_session::AccountSync::ProviderReady { ref provider, .. } if *provider == name));
        let record = world.gateway.provider(&name).ok_or("no provider")?;
        assert_eq!(record.profile.as_str(), "muse");
        assert_eq!(record.access_key, "META_API_KEY");
        assert!(
            profile_env_vars(Agent::MuseCode, Provider::Muse)?.contains(&record.access_key),
            "{}",
            record.access_key
        );
        assert_eq!(record.access_token, MUSE_ACCESS);
        assert_ne!(record.access_token, MUSE_API);
        let debug = format!("{synced:?}");
        assert!(!debug.contains(MUSE_ACCESS), "{debug}");
        assert!(!debug.contains(MUSE_API), "{debug}");
        let seen = security_seen(&world);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].args, SECURITY_ARGS);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn muse_keychain_error_taxonomy_is_typed_and_secret_free() -> TestResult {
    guard(async {
        let cases: Vec<(&str, ProcessReply, bool)> = vec![
            (
                "malformed",
                ProcessReply::exited(0, b"{not json FAKE-MUSE-ACCESS-444", b""),
                false,
            ),
            (
                "missing",
                ProcessReply::exited(
                    0,
                    serde_json::json!({"api_key": MUSE_API})
                        .to_string()
                        .as_bytes(),
                    b"",
                ),
                false,
            ),
            (
                "empty",
                ProcessReply::exited(
                    0,
                    serde_json::json!({"access_token": "  "})
                        .to_string()
                        .as_bytes(),
                    b"",
                ),
                false,
            ),
            (
                "nonzero",
                ProcessReply::exited(44, b"", b"not found"),
                false,
            ),
            ("spawn", ProcessReply::spawn("no binary"), true),
        ];
        for (_name, reply, spawn) in cases {
            let (world, account, _dir) = muse_world("keychain", true).await?;
            world
                .runner
                .scripted()
                .expect(ScriptedProgram::Security, &SECURITY_ARGS, reply);
            let result = world
                .session
                .sync_account(&account, Agent::MuseCode, &CancellationToken::new())
                .await;
            let error = result.err().ok_or("expected error")?;
            if spawn {
                assert!(
                    matches!(error, AccountError::HostLoginStoreUnsupported { .. }),
                    "{error:?}"
                );
            } else {
                assert!(
                    matches!(error, AccountError::MissingCredential { .. }),
                    "{error:?}"
                );
            }
            let text = format!("{error} {error:?}");
            assert!(!text.contains(MUSE_ACCESS), "{text}");
            assert!(!text.contains(MUSE_API), "{text}");
            assert!(world.gateway.providers().is_empty());
            assert_eq!(security_seen(&world).len(), 1);
        }
        let (world, account, _dir) = muse_world("keychain", true).await?;
        let result = world
            .session
            .sync_account(&account, Agent::MuseCode, &CancellationToken::new())
            .await;
        let error = result.err().ok_or("expected error")?;
        assert!(
            matches!(error, AccountError::HostLoginStoreUnsupported { .. }),
            "{error:?}"
        );
        assert!(world.gateway.providers().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn muse_metadata_preconditions_never_run_keychain() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("missing.json");
        let account = host_login_account("museone", Provider::Muse, &HostPath::parse(&file)?)?;
        register(&world, &account).await?;
        let result = world
            .session
            .sync_account(&account, Agent::MuseCode, &CancellationToken::new())
            .await;
        assert!(result.is_err(), "{result:?}");
        assert!(world.gateway.providers().is_empty());
        assert!(security_seen(&world).is_empty());
        for (storage, subs_active, unsupported) in
            [("keychain", false, false), ("os-store", true, true)]
        {
            let (world, account, _dir) = muse_world(storage, subs_active).await?;
            let result = world
                .session
                .sync_account(&account, Agent::MuseCode, &CancellationToken::new())
                .await;
            let error = result.err().ok_or("expected error")?;
            if unsupported {
                assert!(
                    matches!(error, AccountError::HostLoginStoreUnsupported { .. }),
                    "{error:?}"
                );
            } else {
                assert!(
                    matches!(error, AccountError::MissingCredential { .. }),
                    "{error:?}"
                );
            }
            assert!(world.gateway.providers().is_empty());
            assert!(security_seen(&world).is_empty(), "{error:?}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn muse_host_login_account_plans_gateway_sync() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("auth.json");
        let account = host_login_account("museone", Provider::Muse, &HostPath::parse(&file)?)?;
        let plan = plan_account_edits_for_agent(
            &world.desk(),
            vec![ConfigEdit::AddAccount(account)],
            Agent::MuseCode,
            &CancellationToken::new(),
        )
        .await?;
        assert!(plan.needs_gateway());
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
