// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    AccountName, ChatgptAccountId, Clock, CredentialSource, EnvName, EnvSnapshot, FileKind,
    FileMode, FileSystem, HostPath, OpAccountId, OpReference, ProcessRunner, Provider,
};
use jackin_testkit::{FakeClock, MemFs, Method, Route, ScriptedProcessRunner, UsageServer};
use jackin_usage::{
    HostUsage, LimitWindow, ProviderEndpoints, Refresh, Remaining, UsageAccount, UsageDeps,
    UsageError, UsageOutcome, UsageReport, UsageRequest, WindowKind,
};
use tokio_util::sync::CancellationToken;

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) const T0: i64 = 1_767_225_600;

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
        self.build_over(fs, endpoints, state_dir)
    }

    pub(crate) fn host_over(&self, fs: Arc<dyn FileSystem>, state_dir: HostPath) -> HostUsage {
        self.build_over(fs, self.server.endpoints(), state_dir)
    }

    fn build_over(
        &self,
        fs: Arc<dyn FileSystem>,
        endpoints: ProviderEndpoints,
        state_dir: HostPath,
    ) -> HostUsage {
        let clock: Arc<dyn Clock> = Arc::<FakeClock>::clone(&self.clock);
        let runner: Arc<dyn ProcessRunner> = Arc::<ScriptedProcessRunner>::clone(&self.runner);
        HostUsage::new(UsageDeps {
            clock,
            fs,
            runner,
            env: EnvSnapshot::from_pairs(self.env.clone()),
            endpoints,
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
pub(crate) const CODEX_TOKEN: &str = "SECRET_TOKEN_CODEX";
pub(crate) fn codex_body(used: u32) -> String {
    format!(
        "{{\"rate_limit\":{{\"primary_window\":{{\"used_percent\":{used},\"limit_window_seconds\":18000,\"reset_at\":1767232800}}}}}}"
    )
}

pub(crate) const AUTH_CHATGPT: &str = include_str!("fixtures/codex/auth_chatgpt.json");

pub(crate) fn codex_route(body: &str) -> Route {
    Route::json(200, body)
        .requires_header("authorization", &format!("Bearer {CODEX_TOKEN}"))
        .requires_header("chatgpt-account-id", "acct-1")
}

impl UsageHarness {
    pub(crate) async fn with_codex(
        body: &str,
    ) -> Result<(UsageHarness, UsageAccount), Box<dyn Error>> {
        let harness = UsageHarness::new().await?;
        harness
            .put(&format!("{CODEX_HOME}/auth.json"), AUTH_CHATGPT)
            .await?;
        harness
            .server
            .serve(Provider::Codex, Method::Get, CODEX_PATH, codex_route(body));
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
    pub(crate) left: Left,
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
        left,
        resets,
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
        (Some(Remaining::Percent(found)), Left::Percent(wanted)) => assert!(
            near(found.get(), wanted),
            "remaining of {}: {} against {wanted}",
            expected.label,
            found.get()
        ),
        (Some(Remaining::Amount { value, unit }), Left::Amount(wanted, wanted_unit)) => {
            assert_eq!((value.as_str(), unit.as_str()), (wanted, wanted_unit));
        }
        (found, wanted) => panic!(
            "remaining of {}: {found:?} against {wanted:?}",
            expected.label
        ),
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
