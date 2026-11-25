// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use async_trait::async_trait;
use jackin_core::{
    CredentialSource, DirEntry, FileIdentity, FileKind, FileLock, FileMode, FileSystem, FsError,
    HostPath, LinkProblem, Provider, RegularFile, RegularFileProblem, RepoPath, SQLITE_MAGIC,
};
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
);
CREATE TABLE IF NOT EXISTS session_v2 (id TEXT PRIMARY KEY NOT NULL, title TEXT);";

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

fn checkpointed(directory: &Path, rows: &[(&str, &str, i64, &str)]) {
    let db = directory.join("opencode.db");
    let connection = Connection::open(&db).expect("the store opens");
    connection
        .execute_batch("PRAGMA journal_mode=WAL;")
        .expect("the journal mode");
    connection.execute_batch(SCHEMA).expect("the schema");
    for (id, integration, active, value) in rows {
        connection
            .execute(
                "INSERT OR REPLACE INTO credential (id, integration_id, label, value, active, time_created, time_updated) VALUES (?1, ?2, 'opencode', ?3, ?4, 1, 1)",
                rusqlite::params![id, integration, value, active],
            )
            .expect("the row");
    }
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .expect("the checkpoint");
}

#[derive(Debug)]
enum StoreBehaviour {
    Replaced {
        phase: AtomicU32,
    },
    Checkpointed {
        checkpointed: AtomicBool,
        log: Vec<u8>,
    },
}

#[derive(Debug)]
struct StoreFs {
    inner: Arc<dyn FileSystem>,
    store: HostPath,
    before: Vec<u8>,
    after: Vec<u8>,
    behaviour: StoreBehaviour,
    full_reads: AtomicUsize,
}

impl StoreFs {
    fn replaced(
        inner: Arc<dyn FileSystem>,
        store: HostPath,
        before: Vec<u8>,
        after: Vec<u8>,
    ) -> Self {
        StoreFs {
            inner,
            store,
            before,
            after,
            behaviour: StoreBehaviour::Replaced {
                phase: AtomicU32::new(0),
            },
            full_reads: AtomicUsize::new(0),
        }
    }

    fn checkpointed(
        inner: Arc<dyn FileSystem>,
        store: HostPath,
        before: Vec<u8>,
        after: Vec<u8>,
        log: Vec<u8>,
    ) -> Self {
        StoreFs {
            inner,
            store,
            before,
            after,
            behaviour: StoreBehaviour::Checkpointed {
                checkpointed: AtomicBool::new(false),
                log,
            },
            full_reads: AtomicUsize::new(0),
        }
    }

    fn image(&self) -> Vec<u8> {
        match &self.behaviour {
            StoreBehaviour::Replaced { phase } => {
                if phase.load(Ordering::SeqCst) == 0 {
                    self.before.clone()
                } else {
                    self.after.clone()
                }
            }
            StoreBehaviour::Checkpointed { checkpointed, .. } => {
                if checkpointed.load(Ordering::SeqCst) {
                    self.after.clone()
                } else {
                    self.before.clone()
                }
            }
        }
    }

    fn is_store(&self, path: &HostPath) -> bool {
        path.as_path() == self.store.as_path()
            || std::fs::canonicalize(self.store.as_path()).is_ok_and(|real| real == path.as_path())
    }

    fn is_log(path: &HostPath) -> bool {
        path.as_path().to_string_lossy().ends_with("-wal")
    }
}

#[async_trait]
impl FileSystem for StoreFs {
    async fn read(&self, path: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        if self.is_store(path) {
            let image = self.image();
            if let StoreBehaviour::Replaced { phase } = &self.behaviour {
                phase.store(1, Ordering::SeqCst);
            }
            return Ok(Some(image));
        }
        if let StoreBehaviour::Checkpointed { checkpointed, log } = &self.behaviour
            && Self::is_log(path)
        {
            checkpointed.store(true, Ordering::SeqCst);
            return Ok(Some(log.clone()));
        }
        self.inner.read(path).await
    }
    async fn read_at(&self, path: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        if self.is_store(path) {
            let image = self.image();
            let start = (offset as usize).min(image.len());
            let end = start.saturating_add(len).min(image.len());
            return Ok(image[start..end].to_vec());
        }
        self.inner.read_at(path, offset, len).await
    }
    async fn stat(&self, path: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat(path).await
    }
    async fn stat_following(&self, path: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat_following(path).await
    }
    async fn canonicalize(&self, path: &HostPath) -> Result<Option<HostPath>, FsError> {
        self.inner.canonicalize(path).await
    }
    async fn file_len(&self, path: &HostPath) -> Result<Option<u64>, FsError> {
        self.inner.file_len(path).await
    }
    async fn identity(&self, path: &HostPath) -> Result<Option<FileIdentity>, FsError> {
        self.inner.identity(path).await
    }
    async fn list(&self, dir: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        self.inner.list(dir).await
    }
    async fn create_dir_all(&self, dir: &HostPath) -> Result<(), FsError> {
        self.inner.create_dir_all(dir).await
    }
    async fn create_private_dir_all(&self, dir: &HostPath) -> Result<(), FsError> {
        self.inner.create_private_dir_all(dir).await
    }
    async fn write_new(
        &self,
        path: &HostPath,
        bytes: &[u8],
        mode: FileMode,
    ) -> Result<(), FsError> {
        self.inner.write_new(path, bytes, mode).await
    }
    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), FsError> {
        self.inner.rename(from, to).await
    }
    async fn remove_file(&self, path: &HostPath) -> Result<(), FsError> {
        self.inner.remove_file(path).await
    }
    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), FsError> {
        self.inner.remove_dir_all(path).await
    }
    async fn mode(&self, path: &HostPath) -> Result<Option<FileMode>, FsError> {
        self.inner.mode(path).await
    }
    async fn sync_dir(&self, dir: &HostPath) -> Result<(), FsError> {
        self.inner.sync_dir(dir).await
    }
    async fn lock(&self, path: &HostPath) -> Result<FileLock, FsError> {
        self.inner.lock(path).await
    }
    async fn read_link(
        &self,
        root: &HostPath,
        file: &RepoPath,
    ) -> Result<Result<String, LinkProblem>, FsError> {
        self.inner.read_link(root, file).await
    }
    async fn write_symlink(&self, path: &HostPath, target: &str) -> Result<(), FsError> {
        self.inner.write_symlink(path, target).await
    }
    async fn read_regular(
        &self,
        root: &HostPath,
        file: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        let path =
            HostPath::resolve(Path::new(file.as_str()), root).map_err(|error| FsError::Io {
                path: root.clone(),
                kind: std::io::ErrorKind::InvalidInput,
                message: error.to_string(),
            })?;
        let scripted = self.is_store(&path)
            || (matches!(self.behaviour, StoreBehaviour::Checkpointed { .. })
                && Self::is_log(&path));
        if scripted {
            let probe = max_bytes <= SQLITE_MAGIC.len();
            let bytes = if probe && self.is_store(&path) {
                Some(self.image())
            } else {
                self.full_reads.fetch_add(1, Ordering::SeqCst);
                self.read(&path).await?
            };
            return Ok(match bytes {
                Some(mut bytes) => {
                    bytes.truncate(max_bytes.saturating_add(1));
                    Ok(RegularFile {
                        bytes,
                        mode: FileMode(0o600),
                    })
                }
                None => Err(RegularFileProblem::Missing),
            });
        }
        self.inner.read_regular(root, file, max_bytes).await
    }
}

async fn store_report(
    harness: &UsageHarness,
    fs: Arc<dyn FileSystem>,
    store_path: &HostPath,
) -> Result<jackin_usage::UsageOutcome, Box<dyn std::error::Error>> {
    let host = harness.host_over(fs, harness.state_dir.clone());
    let account = account(
        "opencode",
        Provider::Opencode,
        CredentialSource::File {
            path: store_path.clone(),
        },
    );
    let report = harness
        .report_on(&host, std::slice::from_ref(&account), Refresh::Cached)
        .await?;
    Ok(report.accounts[0].outcome.clone())
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
async fn a_store_that_is_checkpointed_while_it_is_read_is_read_again() -> TestResult {
    guard(async {
        const BEFORE: &str = "SECRET_TOKEN_OPENCODE_BEFORE";
        const AFTER: &str = "SECRET_TOKEN_OPENCODE_AFTER";
        let directory = tempfile::tempdir()?;
        let before_dir = directory.path().join("before");
        let after_dir = directory.path().join("after");
        std::fs::create_dir(&before_dir)?;
        std::fs::create_dir(&after_dir)?;
        store(
            &before_dir,
            &[("cred_1", "opencode", 1, &credential(BEFORE, EXPIRES, None))],
        );
        checkpointed(
            &after_dir,
            &[("cred_1", "opencode", 1, &credential(AFTER, EXPIRES, None))],
        );
        let before = std::fs::read(before_dir.join("opencode.db"))?;
        let after = std::fs::read(after_dir.join("opencode.db"))?;
        let harness = UsageHarness::new().await?;
        harness.server.replace(
            Provider::Opencode,
            Method::Get,
            PATH,
            Route::json(200, USAGE).requires_header("authorization", &format!("Bearer {AFTER}")),
        );
        let store_path = host_path_of(&directory.path().join("opencode.db"));
        let store_dir = host_path_of(directory.path());
        harness.fs.create_dir_all(&store_dir).await?;
        harness
            .fs
            .write_new(&store_path, &before, FileMode(0o600))
            .await?;
        let replaced = Arc::new(StoreFs::replaced(
            Arc::<MemFs>::clone(&harness.fs),
            store_path.clone(),
            before,
            after,
        ));
        let fs: Arc<dyn FileSystem> = Arc::<StoreFs>::clone(&replaced);
        let outcome = store_report(&harness, fs, &store_path).await?;
        assert_windows(&outcome, &[FIVE, WEEK, MONTH]);
        assert!(
            replaced.full_reads.load(Ordering::SeqCst) >= 4,
            "the replaced store was not read a second time"
        );
        let mismatches = harness.server.mismatches();
        assert!(mismatches.is_empty(), "{mismatches:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_store_that_is_checkpointed_before_its_log_is_read_is_read_again() -> TestResult {
    guard(async {
        const BEFORE: &str = "SECRET_TOKEN_OPENCODE_BEFORE";
        const AFTER: &str = "SECRET_TOKEN_OPENCODE_AFTER";
        let directory = tempfile::tempdir()?;
        let db = directory.path().join("opencode.db");
        let connection = Connection::open(&db)?;
        connection.execute_batch("PRAGMA journal_mode=WAL;")?;
        connection.execute_batch(SCHEMA)?;
        connection.execute(
            "INSERT OR REPLACE INTO credential (id, integration_id, label, value, active, time_created, time_updated) VALUES ('cred_1', 'opencode', 'opencode', ?1, 1, 1, 1)",
            rusqlite::params![credential(BEFORE, EXPIRES, None)],
        )?;
        connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        let before = std::fs::read(&db)?;
        connection.execute(
            "INSERT OR REPLACE INTO credential (id, integration_id, label, value, active, time_created, time_updated) VALUES ('cred_1', 'opencode', 'opencode', ?1, 1, 2, 2)",
            rusqlite::params![credential(AFTER, EXPIRES, None)],
        )?;
        connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        let after = std::fs::read(&db)?;
        let log = std::fs::read(directory.path().join("opencode.db-wal"))?;
        let harness = UsageHarness::new().await?;
        harness.server.replace(
            Provider::Opencode,
            Method::Get,
            PATH,
            Route::json(200, USAGE).requires_header("authorization", &format!("Bearer {AFTER}")),
        );
        let store_path = host_path_of(&db);
        let store_dir = host_path_of(directory.path());
        harness.fs.create_dir_all(&store_dir).await?;
        harness
            .fs
            .write_new(&store_path, &before, FileMode(0o600))
            .await?;
        let fs: Arc<dyn FileSystem> = Arc::new(StoreFs::checkpointed(
            Arc::<MemFs>::clone(&harness.fs),
            store_path.clone(),
            before,
            after,
            log,
        ));
        let outcome = store_report(&harness, fs, &store_path).await?;
        assert_windows(&outcome, &[FIVE, WEEK, MONTH]);
        let mismatches = harness.server.mismatches();
        assert!(mismatches.is_empty(), "{mismatches:?}");
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

#[tokio::test]
async fn a_store_whose_write_ahead_log_cannot_be_read_is_unreadable() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        store(
            directory.path(),
            &[("cred_1", "opencode", 1, &credential(TOKEN, EXPIRES, None))],
        );
        std::fs::create_dir(directory.path().join("opencode.db-wal"))?;
        let harness = UsageHarness::new().await?;
        let account = account(
            "opencode",
            Provider::Opencode,
            CredentialSource::File {
                path: host_path_of(&directory.path().join("opencode.db")),
            },
        );
        let outcome = harness.report_cached(&account, directory.path()).await?;
        assert!(
            matches!(
                unavailable(&outcome),
                UnavailableReason::Provider(message)
                    if message == "the OpenCode write-ahead log could not be read"
            ),
            "{outcome:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_store_is_never_written_to_a_temporary_file() -> TestResult {
    guard(async {
        const SECRET: &str = "SECRET_TOKEN_OPENCODE_TEMPFREE";
        let directory = tempfile::tempdir()?;
        store(
            directory.path(),
            &[("cred_1", "opencode", 1, &credential(SECRET, EXPIRES, None))],
        );
        let private = directory.path().join("private-temporary-root");
        std::fs::create_dir(&private)?;
        std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o500))?;
        let program = std::env::current_exe()?;
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(
                "exec \"$1\" --exact opencode::the_store_is_read_without_a_temporary_directory --ignored --nocapture",
            )
            .arg("sh")
            .arg(program)
            .env(
                "JACKIN_OPENCODE_STORE",
                directory.path().join("opencode.db"),
            )
            .env("JACKIN_OPENCODE_SECRET", SECRET)
            .env("TMPDIR", &private)
            .env("SQLITE_TMPDIR", &private)
            .output()
            .await?;
        assert!(
            output.status.success(),
            "the store could not be read without a temporary file: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !holds_secret(SECRET, &private),
            "the store was written to a temporary file"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
#[ignore = "the temporary file test runs it with a private temporary directory"]
async fn the_store_is_read_without_a_temporary_directory() -> TestResult {
    let Ok(store) = std::env::var("JACKIN_OPENCODE_STORE") else {
        return Ok(());
    };
    let secret = std::env::var("JACKIN_OPENCODE_SECRET")?;
    let harness = UsageHarness::new().await?;
    harness.server.replace(
        Provider::Opencode,
        Method::Get,
        PATH,
        Route::json(200, USAGE).requires_header("authorization", &format!("Bearer {secret}")),
    );
    let path = std::path::PathBuf::from(store);
    let account = account(
        "opencode",
        Provider::Opencode,
        CredentialSource::File {
            path: host_path_of(&path),
        },
    );
    let outcome = harness
        .report_cached(&account, path.parent().expect("the store directory"))
        .await?;
    assert_windows(&outcome, &[FIVE, WEEK, MONTH]);
    Ok(())
}

#[tokio::test]
async fn an_unrelated_change_in_the_store_keeps_the_cached_report() -> TestResult {
    guard(async {
        let store =
            started(&[("cred_1", "opencode", 1, &credential(TOKEN, EXPIRES, None))]).await?;
        let outcome = store.report().await?;
        assert_windows(&outcome, &[FIVE, WEEK, MONTH]);
        let connection = Connection::open(store.directory.path().join("opencode.db"))?;
        connection.execute(
            "INSERT OR REPLACE INTO session_v2 (id, title) VALUES ('session_1', 'later')",
            [],
        )?;
        drop(connection);
        store.harness.server.replace(
            Provider::Opencode,
            Method::Get,
            PATH,
            Route::json(500, "{}"),
        );
        let again = store.report().await?;
        assert_windows(&again, &[FIVE, WEEK, MONTH]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_store_that_keeps_its_row_in_the_write_ahead_log_is_read() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let db = directory.path().join("opencode.db");
        let connection = Connection::open(&db)?;
        connection.execute_batch("PRAGMA journal_mode=WAL;")?;
        connection.execute_batch(SCHEMA)?;
        connection.execute(
            "INSERT INTO credential (id, integration_id, label, value, active, time_created, time_updated) VALUES ('cred_1', 'opencode', 'opencode', ?1, 1, 1, 1)",
            rusqlite::params![credential(TOKEN, EXPIRES, None)],
        )?;
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
            CredentialSource::File { path: host_path_of(&db) },
        );
        let outcome = harness.report_cached(&account, directory.path()).await?;
        assert_windows(&outcome, &[FIVE, WEEK, MONTH]);
        drop(connection);
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

fn holds_secret(secret: &str, root: &Path) -> bool {
    let needle = secret.as_bytes();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((directory, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let deeper = kind.is_dir() && depth < 2;
            if deeper {
                stack.push((path, depth + 1));
                continue;
            }
            if kind.is_dir() {
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&path)
                && bytes.windows(needle.len()).any(|window| window == needle)
            {
                return true;
            }
        }
    }
    false
}

fn large_page_store(directory: &Path) -> Result<(Connection, std::path::PathBuf), rusqlite::Error> {
    let db = directory.join("opencode.db");
    let connection = Connection::open(&db)?;
    connection.execute_batch(
        "PRAGMA page_size = 65536; PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;",
    )?;
    connection.execute_batch(SCHEMA)?;
    connection.execute(
        "INSERT INTO credential (id, integration_id, label, value, active, time_created, time_updated) VALUES ('cred_1', 'opencode', 'opencode', ?1, 1, 1, 1)",
        rusqlite::params![credential(TOKEN, EXPIRES, None)],
    )?;
    connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    connection.execute(
        "INSERT INTO credential (id, integration_id, label, value, active, time_created, time_updated) VALUES ('cred_2', 'opencode', 'opencode', ?1, 1, 2, 2)",
        rusqlite::params![credential("SECRET_TOKEN_FROM_THE_LOG", EXPIRES, None)],
    )?;
    Ok((connection, directory.join("opencode.db-wal")))
}

async fn report_of_store(
    directory: &Path,
    token: &str,
) -> Result<jackin_usage::UsageOutcome, Box<dyn std::error::Error>> {
    let harness = UsageHarness::new().await?;
    harness.server.replace(
        Provider::Opencode,
        Method::Get,
        PATH,
        Route::json(200, USAGE).requires_header("authorization", &format!("Bearer {token}")),
    );
    let account = account(
        "opencode",
        Provider::Opencode,
        CredentialSource::File {
            path: host_path_of(&directory.join("opencode.db")),
        },
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        harness.report_cached(&account, directory),
    )
    .await
    .map_err(|_| "the report did not finish")?
}

#[tokio::test]
async fn a_log_that_names_a_page_far_beyond_the_store_is_ignored_where_it_cannot_apply()
-> TestResult {
    use crate::support::rewrite_pages;
    guard(async {
        let directory = tempfile::tempdir()?;
        let (connection, log) = large_page_store(directory.path())?;
        let wal = std::fs::read(&log)?;
        assert!(wal.len() >= 32 + 24 + 65_536);
        assert_eq!(
            rewrite_pages(&wal, 65_536, &|_, page, commit| (page, commit)),
            wal
        );
        let last = wal.len() / (24 + 65_536) - 1;
        std::fs::write(
            &log,
            rewrite_pages(&wal, 65_536, &|index, page, commit| {
                if index == last {
                    (u32::MAX, commit)
                } else {
                    (page, commit)
                }
            }),
        )?;
        let outcome = report_of_store(directory.path(), "SECRET_TOKEN_FROM_THE_LOG").await?;
        assert_windows(&outcome, &[FIVE, WEEK, MONTH]);
        drop(connection);
        Ok(())
    })
    .await
}

fn commit_at(index: usize, last: usize, commit: u32, was: u32) -> u32 {
    if index == last { commit } else { was }
}

#[tokio::test]
async fn a_log_that_commits_more_pages_than_the_image_cap_is_unreadable() -> TestResult {
    use crate::support::rewrite_pages;
    guard(async {
        let directory = tempfile::tempdir()?;
        let (connection, log) = large_page_store(directory.path())?;
        let wal = std::fs::read(&log)?;
        let last = wal.len() / (24 + 65_536) - 1;
        for commit in [11_000_u32, u32::MAX] {
            std::fs::write(
                &log,
                rewrite_pages(&wal, 65_536, &|index, page, was| {
                    (page, commit_at(index, last, commit, was))
                }),
            )?;
            let outcome = report_of_store(directory.path(), "SECRET_TOKEN_FROM_THE_LOG").await?;
            assert!(
                matches!(
                    unavailable(&outcome),
                    UnavailableReason::Provider(message)
                        if message == "the OpenCode store could not be read"
                ),
                "commit size {commit}: {outcome:?}"
            );
        }
        drop(connection);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_symlinked_credential_file_that_is_not_the_store_is_read_as_a_credential() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let real = directory.path().join("dotfiles");
        std::fs::create_dir(&real)?;
        std::fs::write(real.join("opencode.json"), credential(TOKEN, EXPIRES, None))?;
        let link = directory.path().join("opencode.json");
        std::os::unix::fs::symlink(real.join("opencode.json"), &link)?;
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
                path: host_path_of(&link),
            },
        );
        let outcome = harness.report_cached(&account, directory.path()).await?;
        assert_windows(&outcome, &[FIVE, WEEK, MONTH]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_symlinked_store_is_refused() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let real = directory.path().join("dotfiles");
        std::fs::create_dir(&real)?;
        store(
            &real,
            &[("cred_1", "opencode", 1, &credential(TOKEN, EXPIRES, None))],
        );
        let link = directory.path().join("opencode.db");
        std::os::unix::fs::symlink(real.join("opencode.db"), &link)?;
        let harness = UsageHarness::new().await?;
        let account = account(
            "opencode",
            Provider::Opencode,
            CredentialSource::File {
                path: host_path_of(&link),
            },
        );
        let outcome = harness.report_cached(&account, directory.path()).await?;
        assert!(
            matches!(
                unavailable(&outcome),
                UnavailableReason::Provider(message)
                    if message == "the OpenCode store could not be read"
            ),
            "{outcome:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_symlink_to_a_pipe_is_refused_without_blocking_the_console() -> TestResult {
    guard(async {
        let directory = tempfile::tempdir()?;
        let fifo = directory.path().join("pipe");
        let made = tokio::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .await?;
        assert!(made.success(), "mkfifo failed");
        let link = directory.path().join("opencode.db");
        std::os::unix::fs::symlink(&fifo, &link)?;
        let harness = UsageHarness::new().await?;
        let account = account(
            "opencode",
            Provider::Opencode,
            CredentialSource::File {
                path: host_path_of(&link),
            },
        );
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            harness.report_cached(&account, directory.path()),
        )
        .await
        .map_err(|_| "the pipe behind the link blocked the console")??;
        assert!(
            matches!(
                unavailable(&outcome),
                UnavailableReason::Provider(message)
                    if message == "the credential file is not a regular file"
            ),
            "{outcome:?}"
        );
        Ok(())
    })
    .await
}

fn padded_credential(total: usize) -> String {
    let base = credential("", EXPIRES, None).len();
    credential(&"k".repeat(total - base), EXPIRES, None)
}

#[tokio::test]
async fn a_credential_value_of_exactly_the_cap_resolves_and_one_byte_more_is_unreadable()
-> TestResult {
    guard(async {
        let cap = 64 * 1024;
        let exact = tempfile::tempdir()?;
        store(
            exact.path(),
            &[("cred_1", "opencode", 1, &padded_credential(cap))],
        );
        let harness = UsageHarness::new().await?;
        harness.server.replace(
            Provider::Opencode,
            Method::Get,
            PATH,
            Route::json(200, USAGE).requires_header(
                "authorization",
                &format!(
                    "Bearer {}",
                    "k".repeat(cap - credential("", EXPIRES, None).len())
                ),
            ),
        );
        let exact_account = account(
            "opencode",
            Provider::Opencode,
            CredentialSource::File {
                path: host_path_of(&exact.path().join("opencode.db")),
            },
        );
        let outcome = harness.report_cached(&exact_account, exact.path()).await?;
        assert_windows(&outcome, &[FIVE, WEEK, MONTH]);

        let over = tempfile::tempdir()?;
        store(
            over.path(),
            &[("cred_1", "opencode", 1, &padded_credential(cap + 1))],
        );
        let over_account = account(
            "opencode",
            Provider::Opencode,
            CredentialSource::File {
                path: host_path_of(&over.path().join("opencode.db")),
            },
        );
        let outcome = harness.report_cached(&over_account, over.path()).await?;
        assert!(
            matches!(
                unavailable(&outcome),
                UnavailableReason::Provider(message)
                    if message == "the OpenCode store could not be read"
            ),
            "{outcome:?}"
        );
        Ok(())
    })
    .await
}
