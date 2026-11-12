// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use jackin_core::{CredentialSource, FileMode, FileSystem, HostPath, Provider};
use jackin_testkit::{MemFs, Method, Route};
use jackin_usage::{Refresh, UnavailableReason, WindowKind};
use rusqlite::Connection;

use crate::support::{
    Left, TestResult, UsageHarness, Want, account, assert_windows, guard, unavailable, want,
};

const PATH: &str = "/zen/go/v1/usage";
const TOKEN: &str = "SECRET_TOKEN_OPENCODE";
const USAGE: &str = include_str!("fixtures/opencode/usage.json");
const EXPIRES: i64 = 1_767_232_800_000;

const FIVE: Want = want(
    WindowKind::FiveHour,
    "5-hour",
    Some(20.0),
    Left::Percent(80.0),
    Some(1_767_243_600),
);
const WEEK: Want = want(
    WindowKind::Weekly,
    "weekly",
    Some(35.0),
    Left::Percent(65.0),
    Some(1_767_830_400),
);
const MONTH: Want = want(
    WindowKind::Monthly,
    "monthly",
    Some(10.0),
    Left::Percent(90.0),
    Some(1_769_817_600),
);

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

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS credential (
    id TEXT PRIMARY KEY NOT NULL,
    integration_id TEXT NOT NULL,
    label TEXT NOT NULL,
    value TEXT NOT NULL,
    connector_id TEXT,
    method_id TEXT,
    active INTEGER,
    time_created INTEGER NOT NULL,
    time_updated INTEGER NOT NULL
);";

fn store(directory: &Path, rows: &[(&str, &str, i64, &str)]) {
    let db = directory.join("opencode.db");
    let connection = Connection::open(&db).expect("the store opens");
    connection.execute_batch(SCHEMA).expect("the schema");
    for (id, integration, active, value) in rows {
        connection
            .execute(
                "INSERT OR REPLACE INTO credential (id, integration_id, label, value, active, time_created, time_updated) VALUES (?1, ?2, 'opencode', ?3, ?4, 1, 1)",
                rusqlite::params![id, integration, value, active],
            )
            .expect("the row");
    }
}

struct Store {
    harness: UsageHarness,
    account: jackin_usage::UsageAccount,
    directory: tempfile::TempDir,
}

async fn started(rows: &[(&str, &str, i64, &str)]) -> Result<Store, Box<dyn std::error::Error>> {
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
    async fn report(&self) -> Result<jackin_usage::UsageOutcome, Box<dyn std::error::Error>> {
        self.harness
            .report_cached(&self.account, self.directory.path())
            .await
    }
}

#[tokio::test]
async fn the_active_console_credential_of_the_store_is_used() -> TestResult {
    guard(async {
        let store =
            started(&[("cred_1", "opencode", 1, &credential(TOKEN, EXPIRES, None))]).await?;
        let outcome = store.report().await?;
        assert_windows(&outcome, &[FIVE, WEEK, MONTH]);
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
            "cred_1",
            "opencode",
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
        assert_windows(&outcome, &[FIVE, WEEK, MONTH]);
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
            started(&[("cred_1", "opencode", 0, &credential(TOKEN, EXPIRES, None))]).await?;
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
        let store = started(&[(
            "cred_1",
            "opencode-go",
            1,
            r#"{"type":"key","key":"a-key"}"#,
        )])
        .await?;
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
            "cred_1",
            "opencode",
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
            "cred_1",
            "opencode",
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

#[tokio::test]
async fn a_store_that_lives_only_in_the_filesystem_boundary_is_read() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        store(
            directory.path(),
            &[("cred_1", "opencode", 1, &credential(TOKEN, EXPIRES, None))],
        );
        let bytes = std::fs::read(directory.path().join("opencode.db"))?;
        let harness = UsageHarness::new().await?;
        harness.server.replace(
            Provider::Opencode,
            Method::Get,
            PATH,
            Route::json(200, USAGE).requires_header("authorization", &format!("Bearer {TOKEN}")),
        );
        let fs: Arc<dyn FileSystem> = Arc::<MemFs>::clone(&harness.fs);
        let store_dir = host_path_of(Path::new("/data/opencode"));
        fs.create_dir_all(&store_dir).await?;
        let path = host_path_of(&store_dir.as_path().join("opencode.db"));
        fs.write_new(&path, &bytes, FileMode(0o600)).await?;
        let account = account(
            "opencode",
            Provider::Opencode,
            CredentialSource::File { path },
        );
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_windows(&usage.outcome, &[FIVE, WEEK, MONTH]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_credential_of_another_kind_is_not_used() -> TestResult {
    guard(async {
        let store =
            started(&[("cred_1", "opencode", 1, r#"{"type":"key","key":"a-key"}"#)]).await?;
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
async fn a_credential_file_that_is_not_the_store_is_read_as_a_credential() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("opencode.json");
        std::fs::write(&path, credential(TOKEN, EXPIRES, None))?;
        let harness = UsageHarness::new().await?;
        harness.server.replace(
            Provider::Opencode,
            Method::Get,
            PATH,
            Route::json(200, USAGE).requires_header("authorization", &format!("Bearer {TOKEN}")),
        );
        let account = account(
            "opencode",
            Provider::Opencode,
            CredentialSource::File {
                path: host_path_of(&path),
            },
        );
        let outcome = harness.report_cached(&account, directory.path()).await?;
        assert_windows(&outcome, &[FIVE, WEEK, MONTH]);
        Ok(())
    })
    .await
}
