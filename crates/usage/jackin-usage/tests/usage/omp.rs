// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use jackin_core::{
    CredentialSource, DirEntry, FileIdentity, FileKind, FileLock, FileMode, FileSystem, FsError,
    HostPath, LinkProblem, Provider, RegularFile, RegularFileProblem, RepoPath, StdFileSystem,
};
use jackin_testkit::{Method, Route, WorkingFs};
use jackin_usage::{Refresh, UnavailableReason, UsageRequest};
use rusqlite::Connection;
use tokio_util::sync::CancellationToken;

use crate::support::{TestResult, UsageHarness, account, guard, unavailable, windows_of};

const PATH: &str = "/api/monitor/usage/quota/limit";
const KEY: &str = "SECRET_TOKEN_OMP";
const KEY_REPLACED: &str = "SECRET_TOKEN_OMP_REPLACED";
const QUOTA: &str = include_str!("fixtures/zai/quota_limit.json");

fn host_path_of(path: &Path) -> HostPath {
    HostPath::parse(path).expect("an absolute path")
}

fn store(home: &Path, rows: &[(i64, &str)]) {
    let directory = home.join(".omp").join("agent");
    std::fs::create_dir_all(&directory).expect("the store directory");
    let db = directory.join("agent.db");
    let connection = Connection::open(&db).expect("the store opens");
    connection
        .execute(
            "CREATE TABLE IF NOT EXISTS auth_credentials (id INTEGER PRIMARY KEY, provider TEXT, credential_type TEXT, data TEXT)",
            [],
        )
        .expect("the table");
    for (id, data) in rows {
        connection
            .execute(
                "INSERT OR REPLACE INTO auth_credentials (id, provider, credential_type, data) VALUES (?1, 'zai', 'api_key', ?2)",
                rusqlite::params![id, data],
            )
            .expect("the row");
    }
}

fn key_json(key: &str) -> String {
    format!("{{\"key\":\"{key}\"}}")
}

struct Omp {
    harness: UsageHarness,
    account: jackin_usage::UsageAccount,
    directory: tempfile::TempDir,
}

async fn started(rows: &[(i64, &str)]) -> Result<Omp, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    store(directory.path(), rows);
    let harness = UsageHarness::new().await?;
    harness.server.replace(
        Provider::Zai,
        Method::Get,
        PATH,
        Route::json(200, QUOTA).requires_header("authorization", KEY),
    );
    let account = account("omp", Provider::Zai, CredentialSource::OmpHost);
    Ok(Omp {
        harness,
        account,
        directory,
    })
}

impl Omp {
    async fn report(&self) -> Result<jackin_usage::UsageOutcome, Box<dyn std::error::Error>> {
        self.harness
            .report_cached(&self.account, self.directory.path())
            .await
    }

    fn host_over(&self, fs: Arc<dyn FileSystem>) -> jackin_usage::HostUsage {
        let state = self.directory.path().join("state").join("usage");
        self.harness.host_on(
            fs,
            host_path_of(&state),
            host_path_of(self.directory.path()),
        )
    }

    async fn report_over(
        &self,
        fs: Arc<dyn FileSystem>,
    ) -> Result<jackin_usage::UsageOutcome, Box<dyn std::error::Error>> {
        let request = UsageRequest {
            accounts: vec![self.account.clone()],
            refresh: Refresh::Cached,
        };
        let mut report = self
            .host_over(fs)
            .report(&request, &CancellationToken::new())
            .await?;
        Ok(report.accounts.remove(0).outcome)
    }
}

#[tokio::test]
async fn an_omp_account_reads_the_key_of_its_store() -> TestResult {
    guard(async {
        let omp = started(&[(1, &key_json(KEY))]).await?;
        let outcome = omp.report().await?;
        assert_eq!(windows_of(&outcome).len(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_omp_credential_that_is_replaced_is_fetched_again() -> TestResult {
    guard(async {
        let omp = started(&[(1, &key_json(KEY))]).await?;
        let outcome = omp.report().await?;
        assert_eq!(windows_of(&outcome).len(), 2);
        store(
            omp.directory.path(),
            &[(1, &key_json(KEY)), (2, &key_json("SECRET_TOKEN_OMP_NEW"))],
        );
        harness_failing(&omp);
        let outcome = omp.report().await?;
        assert!(
            matches!(unavailable(&outcome), UnavailableReason::Provider(_)),
            "the cached windows of the replaced credential were served"
        );
        Ok(())
    })
    .await
}

fn harness_failing(omp: &Omp) {
    omp.harness
        .server
        .replace(Provider::Zai, Method::Get, PATH, Route::status(500));
}

#[tokio::test]
async fn an_omp_store_that_is_recreated_is_fetched_again() -> TestResult {
    guard(async {
        let omp = started(&[(1, &key_json(KEY))]).await?;
        let outcome = omp.report().await?;
        assert_eq!(windows_of(&outcome).len(), 2);
        std::fs::remove_file(
            omp.directory
                .path()
                .join(".omp")
                .join("agent")
                .join("agent.db"),
        )
        .expect("the store is removed");
        store(
            omp.directory.path(),
            &[(1, &key_json("SECRET_TOKEN_OMP_NEW"))],
        );
        harness_failing(&omp);
        let outcome = omp.report().await?;
        assert!(
            matches!(unavailable(&outcome), UnavailableReason::Provider(_)),
            "the cached windows of the recreated store were served"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_omp_store_that_is_recreated_with_a_reused_inode_is_fetched_again() -> TestResult {
    guard(async {
        let omp = started(&[(1, &key_json(KEY))]).await?;
        let db = omp
            .directory
            .path()
            .join(".omp")
            .join("agent")
            .join("agent.db");
        let fs: Arc<dyn FileSystem> = Arc::new(WorkingFs::new(
            Arc::new(ReusedInodeFs::new(
                Arc::new(StdFileSystem::default()),
                host_path_of(&db),
            )),
            &omp.harness.clock,
        ));
        let outcome = omp.report_over(Arc::clone(&fs)).await?;
        assert_eq!(windows_of(&outcome).len(), 2);
        std::fs::remove_file(&db).expect("the store is removed");
        store(
            omp.directory.path(),
            &[(1, &key_json("SECRET_TOKEN_OMP_NEW"))],
        );
        harness_failing(&omp);
        let outcome = omp.report_over(fs).await?;
        assert!(
            matches!(unavailable(&outcome), UnavailableReason::Provider(_)),
            "the cached windows of the recreated store were served"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_identical_omp_store_that_is_recreated_with_a_reused_inode_is_fetched_again()
-> TestResult {
    guard(async {
        let omp = started(&[(1, &key_json(KEY))]).await?;
        let db = omp
            .directory
            .path()
            .join(".omp")
            .join("agent")
            .join("agent.db");
        let fs: Arc<dyn FileSystem> = Arc::new(WorkingFs::new(
            Arc::new(ReusedInodeFs::new(
                Arc::new(StdFileSystem::default()),
                host_path_of(&db),
            )),
            &omp.harness.clock,
        ));
        let outcome = omp.report_over(Arc::clone(&fs)).await?;
        assert_eq!(windows_of(&outcome).len(), 2);
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        std::fs::remove_file(&db).expect("the store is removed");
        store(omp.directory.path(), &[(1, &key_json(KEY))]);
        harness_failing(&omp);
        let outcome = omp.report_over(fs).await?;
        assert!(
            matches!(unavailable(&outcome), UnavailableReason::Provider(_)),
            "the cached windows of the recreated store with unchanged rows were served"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_omp_store_without_a_credential_is_no_credentials() -> TestResult {
    guard(async {
        let omp = started(&[]).await?;
        let outcome = omp.report().await?;
        assert_eq!(unavailable(&outcome), &UnavailableReason::NoCredentials);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_omp_store_that_cannot_be_read_is_a_provider_error() -> TestResult {
    guard(async {
        let omp = started(&[(1, &key_json(KEY))]).await?;
        std::fs::write(
            omp.directory
                .path()
                .join(".omp")
                .join("agent")
                .join("agent.db"),
            "not a database",
        )
        .expect("the store is written");
        let outcome = omp.report().await?;
        assert_eq!(
            unavailable(&outcome),
            &UnavailableReason::Provider("the OMP store could not be read".to_owned())
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_omp_credential_that_is_not_valid_json_is_a_provider_error() -> TestResult {
    guard(async {
        let omp = started(&[(1, "not json")]).await?;
        let outcome = omp.report().await?;
        assert_eq!(
            unavailable(&outcome),
            &UnavailableReason::Provider("the OMP credential is not valid JSON".to_owned())
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_omp_credential_without_a_key_is_a_provider_error() -> TestResult {
    guard(async {
        let omp = started(&[(1, "{}")]).await?;
        let outcome = omp.report().await?;
        assert_eq!(
            unavailable(&outcome),
            &UnavailableReason::Provider("the OMP credential holds no key".to_owned())
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_store_replaced_during_resolution_never_pairs_a_foreign_key() -> TestResult {
    guard(async {
        let omp = started(&[(1, &key_json(KEY))]).await?;
        omp.harness.server.replace(
            Provider::Zai,
            Method::Get,
            PATH,
            Route::json(200, QUOTA).requires_header("authorization", KEY_REPLACED),
        );
        let db = omp
            .directory
            .path()
            .join(".omp")
            .join("agent")
            .join("agent.db");
        let fs: Arc<dyn FileSystem> = Arc::new(WorkingFs::new(
            Arc::new(ReplacingFs::new(
                Arc::new(StdFileSystem::default()),
                host_path_of(&db),
            )),
            &omp.harness.clock,
        ));
        let outcome = omp.report_over(fs).await?;
        assert_eq!(windows_of(&outcome).len(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_omp_store_in_a_directory_that_denies_access_is_a_provider_error() -> TestResult {
    guard(async {
        let omp = started(&[(1, &key_json(KEY))]).await?;
        let directory = omp.directory.path().join(".omp").join("agent");
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o000))
            .expect("the store directory is sealed");
        if std::fs::metadata(directory.join("agent.db")).is_ok() {
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755))
                .expect("the store directory is restored");
            return Err("the store is readable despite mode 000 (running as root?)".into());
        }
        let outcome = omp.report().await?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755))
            .expect("the store directory is restored");
        assert_eq!(
            unavailable(&outcome),
            &UnavailableReason::Provider("the OMP store could not be read".to_owned())
        );
        Ok(())
    })
    .await
}

struct ReplacingFs {
    inner: Arc<dyn FileSystem>,
    target: HostPath,
    identities: AtomicUsize,
}

impl std::fmt::Debug for ReplacingFs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReplacingFs")
    }
}

impl ReplacingFs {
    fn new(inner: Arc<dyn FileSystem>, target: HostPath) -> ReplacingFs {
        ReplacingFs {
            inner,
            target,
            identities: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl FileSystem for ReplacingFs {
    async fn read(&self, path: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        self.inner.read(path).await
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
        if path == &self.target && self.identities.fetch_add(1, Ordering::SeqCst) == 1 {
            replace_store(self.target.as_path(), KEY_REPLACED);
        }
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
    async fn read_at(&self, path: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        self.inner.read_at(path, offset, len).await
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
        self.inner.read_regular(root, file, max_bytes).await
    }
}

struct ReusedInodeFs {
    inner: Arc<dyn FileSystem>,
    target: HostPath,
}

impl std::fmt::Debug for ReusedInodeFs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReusedInodeFs")
    }
}

impl ReusedInodeFs {
    fn new(inner: Arc<dyn FileSystem>, target: HostPath) -> ReusedInodeFs {
        ReusedInodeFs { inner, target }
    }
}

#[async_trait]
impl FileSystem for ReusedInodeFs {
    async fn read(&self, path: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        self.inner.read(path).await
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
        if path == &self.target {
            return Ok(Some(FileIdentity { dev: 1, ino: 42 }));
        }
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
    async fn read_at(&self, path: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        self.inner.read_at(path, offset, len).await
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
        self.inner.read_regular(root, file, max_bytes).await
    }
}

fn replace_store(live: &Path, key: &str) {
    let staged = live.with_extension("staged");
    let connection = Connection::open(&staged).expect("the staged store opens");
    connection
        .execute(
            "CREATE TABLE auth_credentials (id INTEGER PRIMARY KEY, provider TEXT, credential_type TEXT, data TEXT)",
            [],
        )
        .expect("the staged table");
    connection
        .execute(
            "INSERT INTO auth_credentials (id, provider, credential_type, data) VALUES (7, 'zai', 'api_key', ?1)",
            rusqlite::params![key_json(key)],
        )
        .expect("the staged row");
    drop(connection);
    std::fs::rename(&staged, live).expect("the staged store replaces the live store");
}
