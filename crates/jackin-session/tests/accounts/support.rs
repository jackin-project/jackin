// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use jackin_core::{
    Account, AccountName, ChatgptAccountId, CredentialSource, FileMode, FileSystem, HostPath,
    Provider,
};
use jackin_testkit::MemFs;
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
const GUARD: Duration = Duration::from_secs(10);

pub(crate) async fn guard(work: impl Future<Output = TestResult>) -> TestResult {
    tokio::time::timeout(GUARD, work)
        .await
        .map_err(|_| "the test hung for 10 s")?
}

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
