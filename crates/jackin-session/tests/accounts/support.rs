// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::{
    Account, AccountName, ChatgptAccountId, Config, ConfigEdit, CredentialSource, FileLock,
    FileMode, FileSystem, HostPath, Provider,
};
use jackin_testkit::{MemFs, World};
use serde_json::{Value, json};
use tempfile::TempDir;

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) const ACCESS: &str = "ACCESS-SECRET-7f3a91";
pub(crate) const REFRESH: &str = "REFRESH-SECRET-c20e44";
pub(crate) const CLIENT: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub(crate) const ACCOUNT_ID: &str = "3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f";
pub(crate) const API_KEY: &str = "API-SECRET-91b2";
pub(crate) const SECRETS: [&str; 3] = [ACCESS, REFRESH, API_KEY];
const OWNER: &str = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";

pub(crate) fn login_json() -> Value {
    json!({
        "auth_mode": "chatgpt",
        "OPENAI_API_KEY": null,
        "tokens": {
            "id_token": "header.payload.signature",
            "access_token": ACCESS,
            "refresh_token": REFRESH,
            "oauth_provenance": {
                "issuer": "https://auth.openai.com",
                "client_id": CLIENT,
            },
            "login_owner_id": OWNER,
            "account_id": ACCOUNT_ID,
        },
        "last_refresh": "2026-11-02T10:11:12.123456789Z",
    })
}

pub(crate) const GATEWAY: &str = "default_gateway = \"fake-gateway\"\n\n[gateways.fake-gateway]\nopenshell_workspace = \"default\"\nbuilder = \"jackin-builder\"\nplatform = \"linux/arm64\"\n";

pub(crate) async fn register(world: &World, account: &Account) -> TestResult {
    world.seed_all(&Config::parse(GATEWAY)?).await?;
    world
        .config
        .edit(vec![ConfigEdit::AddAccount(account.clone())], &world.cancel)
        .await?;
    Ok(())
}

pub(crate) async fn hold_account(world: &World, name: &str) -> Result<FileLock, Box<dyn Error>> {
    world.fs.create_dir_all(world.cache_dir()).await?;
    let lock = HostPath::resolve(
        std::path::Path::new(&format!("account-{name}.lock")),
        world.cache_dir(),
    )?;
    Ok(world.fs.lock(&lock).await?)
}

pub(crate) async fn until_polling(world: &World, polls: usize) {
    while world.clock.slept().len() < polls {
        tokio::task::yield_now().await;
    }
}

pub(crate) async fn registered() -> Result<World, Box<dyn Error>> {
    let world = World::builder().build()?;
    world.seed_all(&Config::parse(GATEWAY)?).await?;
    Ok(world)
}

pub(crate) fn chatgpt(id: &str) -> Result<ChatgptAccountId, Box<dyn Error>> {
    Ok(ChatgptAccountId::parse(id)?)
}

pub(crate) fn host(path: &str) -> Result<HostPath, Box<dyn Error>> {
    Ok(HostPath::parse(std::path::Path::new(path))?)
}

pub(crate) async fn put(fs: &MemFs, home: &HostPath, text: &str) -> TestResult {
    fs.create_dir_all(home).await?;
    let file = HostPath::parse(&home.as_path().join("auth.json"))?;
    fs.write_new(&file, text.as_bytes(), FileMode(0o600))
        .await?;
    Ok(())
}

pub(crate) fn codex_account(home: &HostPath, account_id: &str) -> Result<Account, Box<dyn Error>> {
    Ok(Account::new(
        AccountName::parse("work")?,
        Provider::Codex,
        CredentialSource::CodexLogin {
            home: home.clone(),
            chatgpt_account_id: ChatgptAccountId::parse(account_id)?,
        },
    )?)
}

pub(crate) struct Home {
    pub(crate) path: HostPath,
    _dir: TempDir,
}

impl Home {
    pub(crate) async fn new(login: &Value) -> Result<Home, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let path = HostPath::parse(&std::fs::canonicalize(dir.path())?)?;
        let home = Home { path, _dir: dir };
        home.write(login).await?;
        Ok(home)
    }

    pub(crate) async fn write(&self, login: &Value) -> TestResult {
        tokio::fs::write(
            self.path.as_path().join("auth.json"),
            login.to_string().as_bytes(),
        )
        .await?;
        Ok(())
    }
}
