// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use jackin_core::{CredentialSource, FileSystem, HostPath, Provider, StdFileSystem};
use jackin_testkit::{Method, Route, WorkingFs};
use jackin_usage::{Refresh, UnavailableReason, UsageRequest};
use rusqlite::Connection;
use tokio_util::sync::CancellationToken;

use crate::support::{TestResult, UsageHarness, account, guard, unavailable, windows_of};

const PATH: &str = "/zen/go/v1/usage";
const TOKEN: &str = "SECRET_TOKEN_OPENCODE";
const USAGE: &str = include_str!("fixtures/opencode/usage.json");
const EXPIRES: i64 = 1_767_232_800_000;

fn host_path_of(path: &Path) -> HostPath {
    HostPath::parse(path).expect("an absolute path")
}

fn credential(access: &str, expires: i64, org: Option<&str>) -> String {
    let metadata = match org {
        Some(org) => format!("{{\"orgID\":\"{org}\",\"email\":\"dev@example.test\"}}"),
        None => "{}".to_owned(),
    };
    format!(
        "{{\"type\":\"oauth\",\"access\":\"{access}\",\"expires\":{expires},\"metadata\":{metadata}}}"
    )
}

fn store(directory: &Path, rows: &[(i64, &str, &str, i64, &str)]) {
    let db = directory.join("opencode.db");
    let connection = Connection::open(&db).expect("the store opens");
    connection
        .execute(
            "CREATE TABLE IF NOT EXISTS credential (id INTEGER PRIMARY KEY, integration_id TEXT NOT NULL, type TEXT NOT NULL, name TEXT, active INTEGER NOT NULL, value TEXT NOT NULL, created_at TEXT, updated_at TEXT)",
            [],
        )
        .expect("the table");
    for (id, integration, kind, active, value) in rows {
        connection
            .execute(
                "INSERT OR REPLACE INTO credential (id, integration_id, type, name, active, value) VALUES (?1, ?2, ?3, 'opencode', ?4, ?5)",
                rusqlite::params![id, integration, kind, active, value],
            )
            .expect("the row");
    }
}

struct Store {
    harness: UsageHarness,
    account: jackin_usage::UsageAccount,
    directory: tempfile::TempDir,
}

async fn started(
    rows: &[(i64, &str, &str, i64, &str)],
) -> Result<Store, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    store(directory.path(), rows);
    let harness = UsageHarness::new().await?;
    harness.server.replace(
        Provider::Opencode,
        Method::Get,
        PATH,
        Route::json(200, USAGE).requires_header("authorization", &format!("Bearer {TOKEN}")),
    );
    let path = host_path_of(&directory.path().join("opencode.db"));
    let account = account(
        "opencode",
        Provider::Opencode,
        CredentialSource::File { path },
    );
    Ok(Store {
        harness,
        account,
        directory,
    })
}

impl Store {
    fn host(&self) -> jackin_usage::HostUsage {
        let fs: Arc<dyn FileSystem> = Arc::new(WorkingFs::new(
            Arc::new(StdFileSystem::default()),
            &self.harness.clock,
        ));
        self.harness.host_on(
            fs,
            host_path_of(&self.directory.path().join("state").join("usage")),
            host_path_of(self.directory.path()),
        )
    }

    async fn report(&self) -> Result<jackin_usage::UsageOutcome, Box<dyn std::error::Error>> {
        let request = UsageRequest {
            accounts: vec![self.account.clone()],
            refresh: Refresh::Cached,
        };
        let mut report = self
            .host()
            .report(&request, &CancellationToken::new())
            .await?;
        Ok(report.accounts.remove(0).outcome)
    }
}

#[tokio::test]
async fn the_active_console_credential_of_the_store_is_used() -> TestResult {
    guard(async {
        let store =
            started(&[(1, "opencode", "oauth", 1, &credential(TOKEN, EXPIRES, None))]).await?;
        let outcome = store.report().await?;
        assert_eq!(windows_of(&outcome).len(), 3);
        let mismatches = store.harness.server.mismatches();
        assert!(mismatches.is_empty(), "{mismatches:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_organization_of_the_active_credential_is_sent_as_a_header() -> TestResult {
    guard(async {
        let store = started(&[(
            1,
            "opencode",
            "oauth",
            1,
            &credential(TOKEN, EXPIRES, Some("org-1")),
        )])
        .await?;
        store.harness.server.replace(
            Provider::Opencode,
            Method::Get,
            PATH,
            Route::json(200, USAGE)
                .requires_header("authorization", &format!("Bearer {TOKEN}"))
                .requires_header("x-opencode-org-id", "org-1"),
        );
        let outcome = store.report().await?;
        assert_eq!(windows_of(&outcome).len(), 3);
        let mismatches = store.harness.server.mismatches();
        assert!(mismatches.is_empty(), "{mismatches:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_inactive_console_credential_is_not_used() -> TestResult {
    guard(async {
        let store =
            started(&[(1, "opencode", "oauth", 0, &credential(TOKEN, EXPIRES, None))]).await?;
        let outcome = store.report().await?;
        assert!(
            matches!(unavailable(&outcome), UnavailableReason::NoCredentials),
            "{outcome:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_store_that_holds_no_console_credential_is_missing() -> TestResult {
    guard(async {
        let store = started(&[(1, "opencode-go", "key", 1, "{}")]).await?;
        let outcome = store.report().await?;
        assert!(
            matches!(unavailable(&outcome), UnavailableReason::NoCredentials),
            "{outcome:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_store_whose_credential_holds_no_access_token_is_a_provider_error() -> TestResult {
    guard(async {
        let store = started(&[(
            1,
            "opencode",
            "oauth",
            1,
            "{\"type\":\"oauth\",\"expires\":1767232800000,\"metadata\":{}}",
        )])
        .await?;
        let outcome = store.report().await?;
        match unavailable(&outcome) {
            UnavailableReason::Provider(message) => {
                assert!(!message.contains("SECRET_TOKEN_OPENCODE"));
            }
            other => panic!("expected a provider reason, found {other:?}"),
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_expired_console_credential_is_an_expired_login() -> TestResult {
    guard(async {
        let store = started(&[(
            1,
            "opencode",
            "oauth",
            1,
            &credential(TOKEN, 1_767_225_599_000, None),
        )])
        .await?;
        let outcome = store.report().await?;
        assert_eq!(unavailable(&outcome), &UnavailableReason::ExpiredLogin);
        Ok(())
    })
    .await
}
