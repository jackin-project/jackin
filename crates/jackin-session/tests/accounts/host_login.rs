// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jackin_core::{
    Account, AccountName, Agent, ConfigEdit, CredentialSource, HostPath, Program, Provider,
    ProviderName, SecretBytes,
};
use jackin_session::{
    ANTIGRAVITY_KEYCHAIN_ARGS, AccountError, CLAUDE_KEYCHAIN_ARGS, parse_claude_keychain_secret,
    plan_account_edits_for_agent, read_claude_login, read_grok_login, read_muse_host_metadata,
};
use jackin_testkit::{ProcessReply, ScriptedProgram, World};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, guard, register};

const GROK_A: &str = "FAKE-GROK-A-111";
const GROK_B: &str = "FAKE-GROK-B-222";
const GROK_REFRESH_A: &str = "FAKE-GROK-REFRESH-A-111";
const GROK_REFRESH_B: &str = "FAKE-GROK-REFRESH-B-222";
const GROK_CLIENT: &str = "grok-client";
const GROK_SCOPE: &str = "https://auth.x.ai::issuer";
const ISSUER: &str = "https://auth.x.ai";
const MUSE_API: &str = "FAKE-MUSE-API-333";
const MUSE_ACCESS: &str = "FAKE-MUSE-ACCESS-444";
const CLAUDE_ACCESS: &str = "FAKE-CLAUDE-ACCESS-555";
const CLAUDE_REFRESH: &str = "FAKE-CLAUDE-REFRESH-666";
const CLAUDE_CLIENT: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDE_EXPIRY: i64 = 1794237825974;
const AGY_ACCESS: &str = "FAKE-AGY-ACCESS-777";
const AGY_REFRESH: &str = "FAKE-AGY-REFRESH-888";
const AGY_PREFIX: &str = "antigravity-store-id";
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

fn grok_document() -> serde_json::Value {
    serde_json::json!({GROK_SCOPE: {
        "key": GROK_A,
        "refresh_token": GROK_REFRESH_A,
        "oidc_client_id": GROK_CLIENT,
        "oidc_issuer": ISSUER,
        "auth_mode": "oidc",
    }})
}

fn grok_login_json(access: &str, refresh: &str) -> String {
    let mut document = grok_document();
    document[GROK_SCOPE]["key"] = serde_json::json!(access);
    document[GROK_SCOPE]["refresh_token"] = serde_json::json!(refresh);
    document.to_string()
}

#[tokio::test]
async fn grok_reader_taxonomy_is_typed_and_secret_free() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        let base = dir.path().join("auth.json");
        let hp = HostPath::parse(&base)?;
        let missing = read_grok_login(world.fs.as_ref(), &hp).await;
        missing.unwrap_err();
        write_file(&base, "{not json").await?;
        let malformed = read_grok_login(world.fs.as_ref(), &hp).await.unwrap_err();
        let mut no_key_document = grok_document();
        no_key_document[GROK_SCOPE]
            .as_object_mut()
            .unwrap()
            .remove("key");
        write_file(&base, &no_key_document.to_string()).await?;
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
            assert!(!text.contains(GROK_REFRESH_A), "{text}");
        }
        assert_ne!(format!("{malformed:?}"), format!("{no_key:?}"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn grok_sync_creates_t03_provider_with_oauth_refresh() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("auth.json");
        write_file(&file, &grok_login_json(GROK_A, GROK_REFRESH_A)).await?;
        let parsed = jackin_core::Config::parse(&format!(
            "[accounts.alpha]\nprovider = \"grok\"\ncredential = {{ source = \"host-login\", path = \"{}\" }}\n",
            file.display()
        ))?;
        assert!(parsed.render().contains("source = \"host-login\""));
        let account = host_login_account("alpha", Provider::Grok, &HostPath::parse(&file)?)?;
        register(&world, &account).await?;
        let login = read_grok_login(world.fs.as_ref(), &HostPath::parse(&file)?).await?;
        assert_eq!(login.issuer, ISSUER);
        assert_eq!(login.access_token.expose(), GROK_A);
        assert_eq!(login.refresh_token.expose(), GROK_REFRESH_A);
        assert_eq!(login.client_id, GROK_CLIENT);
        let login_debug = format!("{login:?}");
        assert!(!login_debug.contains(GROK_A));
        assert!(!login_debug.contains(GROK_REFRESH_A));
        let mut session_document = grok_document();
        session_document[GROK_SCOPE]["auth_mode"] = serde_json::json!("session");
        write_file(&file, &session_document.to_string()).await?;
        read_grok_login(world.fs.as_ref(), &HostPath::parse(&file)?).await?;
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
        assert_eq!(record.refresh_token, GROK_REFRESH_A);
        assert_eq!(record.client_id, GROK_CLIENT);
        assert!(record.scopes.is_empty());
        assert_eq!(record.expires_at, None);
        assert!(record.additional_credentials.is_empty());
        assert_eq!(record.access_key, "GROK_SESSION_TOKEN");
        assert!(
            profile_env_vars(Agent::GrokBuild, Provider::Grok)?.contains(&record.access_key),
            "{}",
            record.access_key
        );
        let debug = format!("{synced:?} {record:?}");
        assert!(!debug.contains(GROK_A));
        assert!(!debug.contains(GROK_REFRESH_A));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn grok_sync_rejects_incomplete_or_wrong_grants_before_publication() -> TestResult {
    guard(async {
        let mut cases = vec![("malformed", None)];
        for field in ["key", "refresh_token", "oidc_client_id"] {
            for replacement in [serde_json::Value::Null, serde_json::json!(" ")] {
                let mut document = grok_document();
                document[GROK_SCOPE][field] = replacement;
                cases.push((field, Some(document)));
            }
        }
        let mut wrong_issuer = grok_document();
        wrong_issuer[GROK_SCOPE]["oidc_issuer"] = serde_json::json!("https://example.com");
        cases.push(("wrong issuer", Some(wrong_issuer)));
        let mut api_key = grok_document();
        api_key[GROK_SCOPE]["auth_mode"] = serde_json::json!("api_key");
        cases.push(("API-key mode", Some(api_key)));
        for (case, document) in cases {
            let world = World::builder().build()?;
            let dir = tempfile::tempdir()?;
            let file = dir.path().join("auth.json");
            let text = document.map_or_else(|| "{not json".to_owned(), |value| value.to_string());
            write_file(&file, &text).await?;
            let path = HostPath::parse(&file)?;
            let direct = read_grok_login(world.fs.as_ref(), &path).await.unwrap_err();
            let account = host_login_account("alpha", Provider::Grok, &path)?;
            register(&world, &account).await?;
            let result = world
                .session
                .sync_account(&account, Agent::GrokBuild, &CancellationToken::new())
                .await;
            let error = result.err().ok_or(case)?;
            for text in [format!("{direct} {direct:?}"), format!("{error} {error:?}")] {
                assert!(!text.contains(GROK_A), "{case}: {text}");
                assert!(!text.contains(GROK_REFRESH_A), "{case}: {text}");
            }
            assert!(world.gateway.profiles().is_empty(), "{case}");
            assert!(world.gateway.providers().is_empty(), "{case}");
        }
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
        read_muse_host_metadata(world.fs.as_ref(), &hp).await.unwrap_err();
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
async fn muse_host_login_account_plans_no_gateway_sync() -> TestResult {
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
        assert!(!plan.needs_gateway());
        plan.apply(&jackin_session::Gateways::single(world.session.clone()))
            .await?;
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
        for (name, token, refresh) in [
            ("alpha", GROK_A, GROK_REFRESH_A),
            ("beta", GROK_B, GROK_REFRESH_B),
        ] {
            let file = dir.path().join(format!("{name}.json"));
            write_file(&file, &grok_login_json(token, refresh)).await?;
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
        let a_record = world.gateway.provider(&a).ok_or("no a")?;
        assert_eq!(a_record.access_token, GROK_A);
        assert_eq!(a_record.refresh_token, GROK_REFRESH_A);
        let b_record = world.gateway.provider(&b).ok_or("no b")?;
        assert_eq!(b_record.access_token, GROK_B);
        assert_eq!(b_record.refresh_token, GROK_REFRESH_B);
        let launch_providers = [ProviderName::for_account(
            &AccountName::parse("beta")?,
            Agent::GrokBuild,
        )];
        assert_eq!(launch_providers, [b]);
        assert!(!format!("{launch_providers:?}").contains(GROK_A));
        assert!(!format!("{launch_providers:?}").contains("alpha"));
        Ok(())
    })
    .await
}

fn claude_document() -> serde_json::Value {
    serde_json::json!({"claudeAiOauth": {
        "accessToken": CLAUDE_ACCESS,
        "refreshToken": CLAUDE_REFRESH,
        "expiresAt": CLAUDE_EXPIRY,
        "scopes": ["user:profile", "user:inference"],
    }})
}

fn claude_file_json() -> String {
    claude_document().to_string()
}

fn claude_keychain_stdout() -> Vec<u8> {
    claude_file_json().into_bytes()
}

fn agy_keychain_stdout() -> Vec<u8> {
    let payload = serde_json::json!({
        "auth_method": "consumer",
        "id_token_email": "operator@example.com",
        "token": {
            "access_token": AGY_ACCESS,
            "token_type": "Bearer",
            "refresh_token": AGY_REFRESH,
            "expiry": "2026-11-09T08:24:53.123456789Z",
            "expires_in": 3599
        }
    });
    let secret = format!(
        "{AGY_PREFIX}:{}",
        URL_SAFE_NO_PAD.encode(payload.to_string())
    );
    format!("{secret}\n").into_bytes()
}

async fn agy_world() -> Result<(World, Account, tempfile::TempDir), Box<dyn std::error::Error>> {
    let world = World::builder().build()?;
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("settings.json");
    write_file(&file, "{}").await?;
    let account = host_login_account("agyone", Provider::Antigravity, &HostPath::parse(&file)?)?;
    register(&world, &account).await?;
    Ok((world, account, dir))
}

#[tokio::test]
async fn claude_reader_taxonomy_is_typed_and_secret_free() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        let base = dir.path().join(".credentials.json");
        let hp = HostPath::parse(&base)?;
        read_claude_login(world.fs.as_ref(), &hp).await.unwrap_err();
        write_file(&base, "{not json").await?;
        let malformed = read_claude_login(world.fs.as_ref(), &hp).await.unwrap_err();
        write_file(
            &base,
            &serde_json::json!({"claudeAiOauth": {"refreshToken": CLAUDE_REFRESH}}).to_string(),
        )
        .await?;
        let no_access = read_claude_login(world.fs.as_ref(), &hp).await.unwrap_err();
        write_file(
            &base,
            &serde_json::json!({"other": {"accessToken": CLAUDE_ACCESS}}).to_string(),
        )
        .await?;
        let no_oauth = read_claude_login(world.fs.as_ref(), &hp).await.unwrap_err();
        let dirpath = dir.path().join("dir.json");
        tokio::fs::create_dir(&dirpath).await?;
        let not_regular = read_claude_login(world.fs.as_ref(), &HostPath::parse(&dirpath)?)
            .await
            .unwrap_err();
        write_file(&base, &"x".repeat(1024 * 1024 + 1)).await?;
        let oversized = read_claude_login(world.fs.as_ref(), &hp).await.unwrap_err();
        for error in [&malformed, &no_access, &no_oauth, &not_regular, &oversized] {
            let text = format!("{error} {error:?}");
            assert!(!text.contains(CLAUDE_ACCESS), "{text}");
            assert!(!text.contains(CLAUDE_REFRESH), "{text}");
        }
        assert_ne!(format!("{malformed:?}"), format!("{no_access:?}"));
        assert_ne!(format!("{no_access:?}"), format!("{no_oauth:?}"));
        assert!(format!("{malformed}").contains(".credentials.json"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn claude_file_sync_creates_t02_provider_with_oauth_refresh() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        let file = dir.path().join(".credentials.json");
        write_file(&file, &claude_file_json()).await?;
        let parsed = jackin_core::Config::parse(&format!(
            "[accounts.claudeone]\nprovider = \"claude\"\ncredential = {{ source = \"host-login\", path = \"{}\" }}\n",
            file.display()
        ))?;
        assert!(parsed.render().contains("source = \"host-login\""));
        let account = host_login_account("claudeone", Provider::Claude, &HostPath::parse(&file)?)?;
        register(&world, &account).await?;
        let login = read_claude_login(world.fs.as_ref(), &HostPath::parse(&file)?).await?;
        assert_eq!(login.access_token.expose(), CLAUDE_ACCESS);
        assert_eq!(login.refresh_token.expose(), CLAUDE_REFRESH);
        assert_eq!(login.expires_at, CLAUDE_EXPIRY);
        assert_eq!(login.scopes, ["user:profile", "user:inference"]);
        let login_debug = format!("{login:?}");
        assert!(!login_debug.contains(CLAUDE_ACCESS));
        assert!(!login_debug.contains(CLAUDE_REFRESH));
        let synced = world
            .session
            .sync_account(&account, Agent::ClaudeCode, &CancellationToken::new())
            .await?;
        let name = ProviderName::for_account(&AccountName::parse("claudeone")?, Agent::ClaudeCode);
        assert_eq!(name.as_str(), "jackin-claudeone-claude");
        assert!(matches!(synced, jackin_session::AccountSync::ProviderReady { ref provider, .. } if *provider == name));
        let record = world.gateway.provider(&name).ok_or("no provider")?;
        assert_eq!(record.profile.as_str(), "claude");
        assert_eq!(record.access_token, CLAUDE_ACCESS);
        assert_eq!(record.refresh_token, CLAUDE_REFRESH);
        assert_eq!(record.client_id, CLAUDE_CLIENT);
        assert_eq!(record.scopes, ["user:profile", "user:inference"]);
        assert_eq!(record.expires_at, Some(CLAUDE_EXPIRY));
        assert!(record.additional_credentials.is_empty());
        assert_eq!(record.access_key, "CLAUDE_CODE_OAUTH_TOKEN");
        assert!(
            profile_env_vars(Agent::ClaudeCode, Provider::Claude)?.contains(&record.access_key),
            "{}",
            record.access_key
        );
        let debug = format!("{synced:?} {record:?}");
        assert!(!debug.contains(CLAUDE_ACCESS), "{debug}");
        assert!(!debug.contains(CLAUDE_REFRESH), "{debug}");
        assert_eq!(security_seen(&world).len(), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn claude_keychain_sync_creates_provider_when_no_file_login_exists() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        let file = dir.path().join(".credentials.json");
        write_file(&file, "{not json").await?;
        let account = host_login_account("claudekey", Provider::Claude, &HostPath::parse(&file)?)?;
        register(&world, &account).await?;
        let login = parse_claude_keychain_secret(&SecretBytes::new(claude_keychain_stdout()))
            .ok_or("no Keychain login")?;
        assert_eq!(login.access_token.expose(), CLAUDE_ACCESS);
        assert_eq!(login.refresh_token.expose(), CLAUDE_REFRESH);
        assert_eq!(login.expires_at, CLAUDE_EXPIRY);
        assert_eq!(login.scopes, ["user:profile", "user:inference"]);
        world.runner.scripted().expect(
            ScriptedProgram::Security,
            &CLAUDE_KEYCHAIN_ARGS,
            ProcessReply::exited(0, &claude_keychain_stdout(), b""),
        );
        let synced = world
            .session
            .sync_account(&account, Agent::ClaudeCode, &CancellationToken::new())
            .await?;
        let name = ProviderName::for_account(&AccountName::parse("claudekey")?, Agent::ClaudeCode);
        assert!(matches!(synced, jackin_session::AccountSync::ProviderReady { ref provider, .. } if *provider == name));
        let record = world.gateway.provider(&name).ok_or("no provider")?;
        assert_eq!(record.profile.as_str(), "claude");
        assert_eq!(record.access_token, CLAUDE_ACCESS);
        assert_eq!(record.refresh_token, CLAUDE_REFRESH);
        assert_eq!(record.client_id, CLAUDE_CLIENT);
        assert_eq!(record.scopes, ["user:profile", "user:inference"]);
        assert_eq!(record.expires_at, Some(CLAUDE_EXPIRY));
        assert!(record.additional_credentials.is_empty());
        assert_eq!(record.access_key, "CLAUDE_CODE_OAUTH_TOKEN");
        let seen = security_seen(&world);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].args, CLAUDE_KEYCHAIN_ARGS);
        let debug = format!("{synced:?} {record:?}");
        assert!(!debug.contains(CLAUDE_ACCESS));
        assert!(!debug.contains(CLAUDE_REFRESH));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn claude_sync_rejects_incomplete_grants_before_publication() -> TestResult {
    guard(async {
        let mut cases = vec![("malformed", None)];
        for field in ["accessToken", "refreshToken"] {
            for replacement in [serde_json::Value::Null, serde_json::json!(" ")] {
                let mut document = claude_document();
                document["claudeAiOauth"][field] = replacement;
                cases.push((field, Some(document)));
            }
        }
        for expiry in [
            serde_json::Value::Null,
            serde_json::json!(0),
            serde_json::json!(-1),
            serde_json::json!("1794237825974"),
            serde_json::json!(1.5),
            serde_json::json!(u64::MAX),
        ] {
            let mut document = claude_document();
            document["claudeAiOauth"]["expiresAt"] = expiry;
            cases.push(("expiry", Some(document)));
        }
        for scopes in [
            serde_json::Value::Null,
            serde_json::json!([]),
            serde_json::json!(["user:profile", " "]),
            serde_json::json!("user:profile"),
        ] {
            let mut document = claude_document();
            document["claudeAiOauth"]["scopes"] = scopes;
            cases.push(("scopes", Some(document)));
        }
        cases.push((
            "access only",
            Some(serde_json::json!({"claudeAiOauth": {"accessToken": CLAUDE_ACCESS}})),
        ));
        cases.push((
            "API key",
            Some(serde_json::json!({"ANTHROPIC_API_KEY": CLAUDE_ACCESS})),
        ));
        for (case, document) in cases {
            let world = World::builder().build()?;
            let dir = tempfile::tempdir()?;
            let file = dir.path().join(".credentials.json");
            let text = document.map_or_else(|| "{not json".to_owned(), |value| value.to_string());
            write_file(&file, &text).await?;
            let path = HostPath::parse(&file)?;
            let direct = read_claude_login(world.fs.as_ref(), &path)
                .await
                .unwrap_err();
            assert!(
                parse_claude_keychain_secret(&SecretBytes::new(text.into_bytes())).is_none(),
                "{case}"
            );
            let account = host_login_account("claudeone", Provider::Claude, &path)?;
            register(&world, &account).await?;
            let result = world
                .session
                .sync_account(&account, Agent::ClaudeCode, &CancellationToken::new())
                .await;
            let error = result.err().ok_or(case)?;
            for text in [format!("{direct} {direct:?}"), format!("{error} {error:?}")] {
                assert!(!text.contains(CLAUDE_ACCESS), "{case}: {text}");
                assert!(!text.contains(CLAUDE_REFRESH), "{case}: {text}");
            }
            assert!(world.gateway.profiles().is_empty(), "{case}");
            assert!(world.gateway.providers().is_empty(), "{case}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn claude_keychain_error_taxonomy_is_typed_and_secret_free() -> TestResult {
    guard(async {
        let cases: Vec<(&str, ProcessReply, bool)> = vec![
            (
                "malformed",
                ProcessReply::exited(0, b"{not json FAKE-CLAUDE-ACCESS-555", b""),
                false,
            ),
            (
                "missing-oauth",
                ProcessReply::exited(
                    0,
                    serde_json::json!({"other": {"accessToken": CLAUDE_ACCESS}})
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
                    serde_json::json!({"claudeAiOauth": {"accessToken": "  "}})
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
            let world = World::builder().build()?;
            let dir = tempfile::tempdir()?;
            let file = dir.path().join(".credentials.json");
            let account =
                host_login_account("claudekey", Provider::Claude, &HostPath::parse(&file)?)?;
            register(&world, &account).await?;
            world
                .runner
                .scripted()
                .expect(ScriptedProgram::Security, &CLAUDE_KEYCHAIN_ARGS, reply);
            let result = world
                .session
                .sync_account(&account, Agent::ClaudeCode, &CancellationToken::new())
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
            assert!(!text.contains(CLAUDE_ACCESS), "{text}");
            assert!(!text.contains(CLAUDE_REFRESH), "{text}");
            assert!(world.gateway.providers().is_empty());
            assert_eq!(security_seen(&world).len(), 1);
        }
        let world = World::builder().build()?;
        let dir = tempfile::tempdir()?;
        let file = dir.path().join(".credentials.json");
        let account = host_login_account("claudekey", Provider::Claude, &HostPath::parse(&file)?)?;
        register(&world, &account).await?;
        let result = world
            .session
            .sync_account(&account, Agent::ClaudeCode, &CancellationToken::new())
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
async fn antigravity_keychain_sync_creates_t05_provider() -> TestResult {
    guard(async {
        let (world, account, _dir) = agy_world().await?;
        world.runner.scripted().expect(
            ScriptedProgram::Security,
            &ANTIGRAVITY_KEYCHAIN_ARGS,
            ProcessReply::exited(0, &agy_keychain_stdout(), b""),
        );
        let synced = world
            .session
            .sync_account(&account, Agent::Antigravity, &CancellationToken::new())
            .await?;
        let name = ProviderName::for_account(&AccountName::parse("agyone")?, Agent::Antigravity);
        assert_eq!(name.as_str(), "jackin-agyone-agy");
        assert!(matches!(synced, jackin_session::AccountSync::ProviderReady { ref provider, .. } if *provider == name));
        let record = world.gateway.provider(&name).ok_or("no provider")?;
        assert_eq!(record.profile.as_str(), "agy");
        assert_eq!(record.access_token, AGY_ACCESS);
        assert_eq!(record.access_key, "GOOGLE_API_KEY");
        assert!(
            profile_env_vars(Agent::Antigravity, Provider::Antigravity)?
                .contains(&record.access_key),
            "{}",
            record.access_key
        );
        let debug = format!("{synced:?}");
        assert!(!debug.contains(AGY_ACCESS), "{debug}");
        assert!(!debug.contains(AGY_REFRESH), "{debug}");
        assert!(!debug.contains("operator@example.com"), "{debug}");
        let seen = security_seen(&world);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].args, ANTIGRAVITY_KEYCHAIN_ARGS);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn antigravity_keychain_error_taxonomy_is_typed_and_secret_free() -> TestResult {
    guard(async {
        let encoded = |payload: serde_json::Value| URL_SAFE_NO_PAD.encode(payload.to_string());
        let cases: Vec<(&str, ProcessReply, bool)> = vec![
            (
                "no-colon",
                ProcessReply::exited(0, format!("{AGY_ACCESS}x").as_bytes(), b""),
                false,
            ),
            (
                "bad-base64",
                ProcessReply::exited(
                    0,
                    format!("{AGY_PREFIX}:!!not-base64-{AGY_ACCESS}").as_bytes(),
                    b"",
                ),
                false,
            ),
            (
                "missing-token",
                ProcessReply::exited(
                    0,
                    format!(
                        "{AGY_PREFIX}:{}",
                        encoded(serde_json::json!({"auth_method": "consumer"}))
                    )
                    .as_bytes(),
                    b"",
                ),
                false,
            ),
            (
                "empty",
                ProcessReply::exited(
                    0,
                    format!(
                        "{AGY_PREFIX}:{}",
                        encoded(serde_json::json!({"token": {"access_token": " "}}))
                    )
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
            let (world, account, _dir) = agy_world().await?;
            world.runner.scripted().expect(
                ScriptedProgram::Security,
                &ANTIGRAVITY_KEYCHAIN_ARGS,
                reply,
            );
            let result = world
                .session
                .sync_account(&account, Agent::Antigravity, &CancellationToken::new())
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
            assert!(!text.contains(AGY_ACCESS), "{text}");
            assert!(!text.contains(AGY_REFRESH), "{text}");
            assert!(world.gateway.providers().is_empty());
            assert_eq!(security_seen(&world).len(), 1);
        }
        let (world, account, _dir) = agy_world().await?;
        let result = world
            .session
            .sync_account(&account, Agent::Antigravity, &CancellationToken::new())
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
async fn claude_and_antigravity_host_login_accounts_plan_no_gateway_sync() -> TestResult {
    guard(async {
        for (provider, agent, tuple_id) in [
            (Provider::Claude, Agent::ClaudeCode, "T02"),
            (Provider::Antigravity, Agent::Antigravity, "T05"),
        ] {
            let world = World::builder().build()?;
            let dir = tempfile::tempdir()?;
            let file = dir.path().join("login.json");
            let account = host_login_account("laneone", provider, &HostPath::parse(&file)?)?;
            let plan = plan_account_edits_for_agent(
                &world.desk(),
                vec![ConfigEdit::AddAccount(account)],
                agent,
                &CancellationToken::new(),
            )
            .await?;
            assert!(!plan.needs_gateway());
            plan.apply(&jackin_session::Gateways::single(world.session.clone()))
                .await?;
            assert!(world.gateway.providers().is_empty(), "{tuple_id}");
        }
        Ok(())
    })
    .await
}
