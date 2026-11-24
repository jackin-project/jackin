// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    AccountName, ChatgptAccountId, Clock, CredentialSource, EnvName, EnvSnapshot, FileKind,
    FileMode, FileSystem, HostPath, OpAccountId, OpReference, ProcessRunner, Provider,
    StdFileSystem,
};
use jackin_testkit::{
    FakeClock, MemFs, Method, Route, ScriptedProcessRunner, UsageServer, WorkingFs,
};
use jackin_usage::{
    HostUsage, LimitWindow, ProviderEndpoints, Refresh, Remaining, UsageAccount, UsageDeps,
    UsageError, UsageOutcome, UsageReport, UsageRequest, WindowKind,
};
use tokio_util::sync::CancellationToken;

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) const T0: i64 = 1_767_225_600;
const GUARD: Duration = Duration::from_secs(20);

pub(crate) async fn guard(work: impl Future<Output = TestResult>) -> TestResult {
    tokio::time::timeout(GUARD, work)
        .await
        .map_err(|_| "the test hung for 20 s")?
}

pub(crate) fn host_path(path: &str) -> HostPath {
    HostPath::parse(Path::new(path)).expect("absolute path")
}

pub(crate) struct UsageHarness {
    pub(crate) fs: Arc<MemFs>,
    pub(crate) clock: Arc<FakeClock>,
    pub(crate) runner: Arc<ScriptedProcessRunner>,
    pub(crate) server: UsageServer,
    pub(crate) state_dir: HostPath,
    env: Vec<(String, String)>,
}

impl UsageHarness {
    pub(crate) async fn new() -> Result<UsageHarness, Box<dyn Error>> {
        Ok(UsageHarness {
            fs: Arc::new(MemFs::new()),
            clock: Arc::new(FakeClock::new()),
            runner: Arc::new(ScriptedProcessRunner::new()),
            server: UsageServer::start().await?,
            state_dir: host_path("/state/usage"),
            env: Vec::new(),
        })
    }

    pub(crate) fn env(&mut self, name: &str, value: &str) {
        self.env.push((name.to_owned(), value.to_owned()));
    }

    pub(crate) fn set_env(&mut self, name: &str, value: &str) {
        self.env.retain(|(key, _)| key != name);
        self.env.push((name.to_owned(), value.to_owned()));
    }

    pub(crate) fn host(&self) -> HostUsage {
        self.host_with(self.server.endpoints())
    }

    pub(crate) fn host_in(&self, state_dir: &HostPath) -> HostUsage {
        self.build(self.server.endpoints(), state_dir.clone())
    }

    pub(crate) fn host_with(&self, endpoints: ProviderEndpoints) -> HostUsage {
        self.build(endpoints, self.state_dir.clone())
    }

    fn build(&self, endpoints: ProviderEndpoints, state_dir: HostPath) -> HostUsage {
        let fs: Arc<dyn FileSystem> = Arc::<MemFs>::clone(&self.fs);
        self.build_over(fs, endpoints, state_dir, host_path("/home/work"))
    }

    pub(crate) fn host_over(&self, fs: Arc<dyn FileSystem>, state_dir: HostPath) -> HostUsage {
        self.build_over(
            fs,
            self.server.endpoints(),
            state_dir,
            host_path("/home/work"),
        )
    }

    pub(crate) fn host_on(
        &self,
        fs: Arc<dyn FileSystem>,
        state_dir: HostPath,
        home: HostPath,
    ) -> HostUsage {
        self.build_over(fs, self.server.endpoints(), state_dir, home)
    }

    fn build_over(
        &self,
        fs: Arc<dyn FileSystem>,
        endpoints: ProviderEndpoints,
        state_dir: HostPath,
        home: HostPath,
    ) -> HostUsage {
        let clock: Arc<dyn Clock> = Arc::<FakeClock>::clone(&self.clock);
        let runner: Arc<dyn ProcessRunner> = Arc::<ScriptedProcessRunner>::clone(&self.runner);
        HostUsage::new(UsageDeps {
            clock,
            fs,
            runner,
            env: EnvSnapshot::from_pairs(self.env.clone()),
            endpoints,
            home,
            state_dir,
        })
    }

    pub(crate) async fn report(
        &self,
        accounts: &[UsageAccount],
        refresh: Refresh,
    ) -> Result<UsageReport, UsageError> {
        self.report_on(&self.host(), accounts, refresh).await
    }

    pub(crate) async fn report_on(
        &self,
        host: &HostUsage,
        accounts: &[UsageAccount],
        refresh: Refresh,
    ) -> Result<UsageReport, UsageError> {
        let request = UsageRequest {
            accounts: accounts.to_vec(),
            refresh,
        };
        host.report(&request, &CancellationToken::new()).await
    }

    pub(crate) async fn one(
        &self,
        account: &UsageAccount,
        refresh: Refresh,
    ) -> Result<jackin_usage::AccountUsage, UsageError> {
        let mut report = self.report(std::slice::from_ref(account), refresh).await?;
        Ok(report.accounts.remove(0))
    }

    pub(crate) fn working_host(&self, directory: &Path) -> HostUsage {
        let fs: Arc<dyn FileSystem> = Arc::new(WorkingFs::new(
            Arc::new(StdFileSystem::default()),
            &self.clock,
        ));
        self.host_on(
            fs,
            HostPath::parse(&directory.join("state").join("usage")).expect("an absolute path"),
            HostPath::parse(directory).expect("an absolute path"),
        )
    }

    pub(crate) async fn report_cached(
        &self,
        account: &UsageAccount,
        directory: &Path,
    ) -> Result<UsageOutcome, Box<dyn Error>> {
        let host = self.working_host(directory);
        let mut report = self
            .report_on(&host, std::slice::from_ref(account), Refresh::Cached)
            .await?;
        Ok(report.accounts.remove(0).outcome)
    }

    pub(crate) async fn put(&self, path: &str, text: &str) -> TestResult {
        let path = host_path(path);
        if let Some(parent) = path.as_path().parent() {
            self.fs.create_dir_all(&HostPath::parse(parent)?).await?;
        }
        self.fs
            .write_new(&path, text.as_bytes(), FileMode(0o600))
            .await?;
        Ok(())
    }

    pub(crate) fn advance(&self, seconds: u64) {
        self.clock.advance(Duration::from_secs(seconds));
    }

    pub(crate) fn cache_text(&self) -> Option<String> {
        self.fs.read_string(&host_path("/state/usage/usage.json"))
    }

    pub(crate) async fn all_files(&self) -> Result<Vec<(String, Vec<u8>)>, Box<dyn Error>> {
        let mut out = Vec::new();
        let mut pending = vec![host_path("/")];
        while let Some(dir) = pending.pop() {
            for entry in self.fs.list(&dir).await? {
                let path = HostPath::parse(&dir.as_path().join(&entry.name))?;
                match entry.kind {
                    FileKind::Dir => pending.push(path),
                    FileKind::File => {
                        let bytes = self.fs.read(&path).await?.unwrap_or_default();
                        out.push((path.as_path().display().to_string(), bytes));
                    }
                    FileKind::Symlink | FileKind::Socket | FileKind::Other => {}
                }
            }
        }
        out.sort();
        Ok(out)
    }
}

pub(crate) fn account(name: &str, provider: Provider, source: CredentialSource) -> UsageAccount {
    UsageAccount {
        name: AccountName::parse(name).expect("account name"),
        provider,
        credential: source,
    }
}

pub(crate) fn env_source(var: &str) -> CredentialSource {
    CredentialSource::Env {
        var: EnvName::parse(var).expect("env name"),
    }
}

pub(crate) fn op_source(reference: &str) -> CredentialSource {
    CredentialSource::OnePassword {
        reference: OpReference::parse(reference).expect("op reference"),
        account: None,
    }
}

pub(crate) fn op_source_in(reference: &str, account: &str) -> CredentialSource {
    CredentialSource::OnePassword {
        reference: OpReference::parse(reference).expect("op reference"),
        account: Some(OpAccountId::parse(account).expect("op account")),
    }
}

pub(crate) fn file_source(path: &str) -> CredentialSource {
    CredentialSource::File {
        path: host_path(path),
    }
}

pub(crate) fn codex_login(name: &str, home: &str, account_id: &str) -> UsageAccount {
    account(
        name,
        Provider::Codex,
        CredentialSource::CodexLogin {
            home: host_path(home),
            chatgpt_account_id: ChatgptAccountId::parse(account_id).expect("account id"),
        },
    )
}

pub(crate) const CODEX_HOME: &str = "/home/work/.codex";
pub(crate) const CODEX_PATH: &str = "/backend-api/wham/usage";
pub(crate) const RESET_CREDITS_PATH: &str = "/backend-api/wham/rate-limit-reset-credits";
pub(crate) const RESET_CREDITS: &str = "{\"available_count\":0}";
pub(crate) const CODEX_TOKEN: &str = "SECRET_TOKEN_CODEX";
pub(crate) fn codex_body(used: u32) -> String {
    format!(
        "{{\"rate_limit\":{{\"primary_window\":{{\"used_percent\":{used},\"limit_window_seconds\":18000,\"reset_at\":1767232800}}}}}}"
    )
}

pub(crate) const AUTH_CHATGPT: &str = include_str!("fixtures/codex/auth_chatgpt.json");

pub(crate) fn codex_auth(account_id: &str) -> String {
    format!(
        "{{\"auth_mode\":\"chatgpt\",\"OPENAI_API_KEY\":null,\"tokens\":{{\"id_token\":\"x\",\"access_token\":\"{CODEX_TOKEN}\",\"refresh_token\":\"SECRET_TOKEN_CODEX_REFRESH\",\"account_id\":\"{account_id}\"}},\"last_refresh\":\"2026-01-01T00:00:00Z\"}}"
    )
}

pub(crate) async fn put_codex_auth(
    harness: &UsageHarness,
    home: &str,
    account_id: &str,
) -> TestResult {
    harness
        .put(&format!("{home}/auth.json"), &codex_auth(account_id))
        .await
}

pub(crate) fn codex_route(body: &str) -> Route {
    Route::json(200, body)
        .requires_header("authorization", &format!("Bearer {CODEX_TOKEN}"))
        .requires_header("chatgpt-account-id", "acct-1")
}

pub(crate) fn codex_reset_credits_route(token: &str) -> Route {
    Route::json(200, RESET_CREDITS).requires_header("authorization", &format!("Bearer {token}"))
}

pub(crate) fn serve_codex(harness: &UsageHarness, token: &str, usage: Route) {
    harness
        .server
        .serve(Provider::Codex, Method::Get, CODEX_PATH, usage);
    harness.server.serve(
        Provider::Codex,
        Method::Get,
        RESET_CREDITS_PATH,
        codex_reset_credits_route(token),
    );
}

impl UsageHarness {
    pub(crate) async fn with_codex(
        body: &str,
    ) -> Result<(UsageHarness, UsageAccount), Box<dyn Error>> {
        let harness = UsageHarness::new().await?;
        harness
            .put(&format!("{CODEX_HOME}/auth.json"), AUTH_CHATGPT)
            .await?;
        serve_codex(&harness, CODEX_TOKEN, codex_route(body));
        Ok((harness, codex_login("work", CODEX_HOME, "acct-1")))
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Left {
    Percent(f64),
    Amount(&'static str, &'static str),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Want {
    pub(crate) kind: WindowKind,
    pub(crate) label: &'static str,
    pub(crate) used: Option<f64>,
    pub(crate) left: Option<Left>,
    pub(crate) spent: Option<Left>,
    pub(crate) resets: Option<i64>,
}

pub(crate) const fn want(
    kind: WindowKind,
    label: &'static str,
    used: Option<f64>,
    left: Left,
    resets: Option<i64>,
) -> Want {
    Want {
        kind,
        label,
        used,
        left: Some(left),
        spent: None,
        resets,
    }
}

pub(crate) const fn spent(kind: WindowKind, label: &'static str, spent: Left) -> Want {
    Want {
        kind,
        label,
        used: None,
        left: None,
        spent: Some(spent),
        resets: None,
    }
}

fn near(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < 1e-9
}

pub(crate) fn assert_window(actual: &LimitWindow, expected: &Want) {
    assert_eq!(actual.kind, expected.kind, "kind of {}", expected.label);
    assert_eq!(actual.label, expected.label, "label");
    match (actual.used, expected.used) {
        (None, None) => {}
        (Some(found), Some(wanted)) => assert!(
            near(found.get(), wanted),
            "used of {}: {} against {wanted}",
            expected.label,
            found.get()
        ),
        (found, wanted) => panic!("used of {}: {found:?} against {wanted:?}", expected.label),
    }
    match (&actual.remaining, expected.left) {
        (Some(Remaining::Percent(found)), Some(Left::Percent(wanted))) => assert!(
            near(found.get(), wanted),
            "remaining of {}: {} against {wanted}",
            expected.label,
            found.get()
        ),
        (Some(Remaining::Amount { value, unit }), Some(Left::Amount(wanted, wanted_unit))) => {
            assert_eq!((value.as_str(), unit.as_str()), (wanted, wanted_unit));
        }
        (None, None) => {}
        (found, wanted) => panic!(
            "remaining of {}: {found:?} against {wanted:?}",
            expected.label
        ),
    }
    match (&actual.spent, expected.spent) {
        (None, None) => {}
        (Some(Remaining::Percent(found)), Some(Left::Percent(wanted))) => assert!(
            near(found.get(), wanted),
            "spent of {}: {} against {wanted}",
            expected.label,
            found.get()
        ),
        (Some(Remaining::Amount { value, unit }), Some(Left::Amount(wanted, wanted_unit))) => {
            assert_eq!((value.as_str(), unit.as_str()), (wanted, wanted_unit));
        }
        (found, wanted) => panic!("spent of {}: {found:?} against {wanted:?}", expected.label),
    }
    assert_eq!(
        actual.resets_at.map(|at| at.0),
        expected.resets,
        "resets_at of {}",
        expected.label
    );
}

pub(crate) fn windows_of(outcome: &UsageOutcome) -> &[LimitWindow] {
    match outcome {
        UsageOutcome::Windows(windows) => windows,
        UsageOutcome::Unavailable(reason) => panic!("expected windows, found {reason:?}"),
    }
}

pub(crate) fn assert_windows(outcome: &UsageOutcome, expected: &[Want]) {
    let actual = windows_of(outcome);
    assert_eq!(
        actual.len(),
        expected.len(),
        "window labels {:?}",
        actual.iter().map(|w| w.label.as_str()).collect::<Vec<_>>()
    );
    for (window, wanted) in actual.iter().zip(expected) {
        assert_window(window, wanted);
    }
}

pub(crate) async fn assert_only_state_written(
    harness: &UsageHarness,
    setup: &[(&str, &str)],
) -> TestResult {
    let files = harness.all_files().await?;
    let mut expected: Vec<String> = setup.iter().map(|(path, _)| (*path).to_owned()).collect();
    expected.push("/state/usage/usage.json".to_owned());
    expected.push("/state/usage/usage.json.lock".to_owned());
    expected.sort();
    let names: Vec<String> = files.iter().map(|(path, _)| path.clone()).collect();
    assert_eq!(names, expected, "paths in the filesystem");
    for (path, text) in setup {
        let found = files.iter().find(|(name, _)| name == path);
        assert_eq!(
            found.map(|(_, bytes)| bytes.as_slice()),
            Some(text.as_bytes()),
            "bytes of {path}"
        );
    }
    Ok(())
}

pub(crate) fn unavailable(outcome: &UsageOutcome) -> &jackin_usage::UnavailableReason {
    match outcome {
        UsageOutcome::Unavailable(reason) => reason,
        UsageOutcome::Windows(windows) => {
            panic!("expected a reason, found {} windows", windows.len())
        }
    }
}

pub(crate) fn wal_checksum(
    data: &[u8],
    mut one: u32,
    mut two: u32,
    big_endian: bool,
) -> (u32, u32) {
    let (pairs, _) = data.as_chunks::<8>();
    for pair in pairs {
        let word = |bytes: &[u8]| {
            let array: [u8; 4] = bytes.try_into().expect("four bytes");
            if big_endian {
                u32::from_be_bytes(array)
            } else {
                u32::from_le_bytes(array)
            }
        };
        one = one.wrapping_add(word(&pair[0..4])).wrapping_add(two);
        two = two.wrapping_add(word(&pair[4..8])).wrapping_add(one);
    }
    (one, two)
}

pub(crate) fn rewrite_pages(
    wal: &[u8],
    page_size: usize,
    set: &dyn Fn(usize, u32) -> u32,
) -> Vec<u8> {
    let big_endian = wal[3] & 1 == 1;
    let mut out = wal.to_vec();
    let header = wal[0..24].to_vec();
    let (mut one, mut two) = wal_checksum(&header, 0, 0, big_endian);
    let frame = 24 + page_size;
    let mut index = 0;
    let mut offset = 32;
    while offset + frame <= out.len() && out[offset + 8..offset + 16] == wal[16..24] {
        let page = u32::from_be_bytes(out[offset..offset + 4].try_into().expect("page"));
        out[offset..offset + 4].copy_from_slice(&set(index, page).to_be_bytes());
        let (next_one, next_two) = wal_checksum(&out[offset..offset + 8], one, two, big_endian);
        let (next_one, next_two) = wal_checksum(
            &out[offset + 24..offset + frame],
            next_one,
            next_two,
            big_endian,
        );
        out[offset + 16..offset + 20].copy_from_slice(&next_one.to_be_bytes());
        out[offset + 20..offset + 24].copy_from_slice(&next_two.to_be_bytes());
        (one, two) = (next_one, next_two);
        index += 1;
        offset += frame;
    }
    out
}
