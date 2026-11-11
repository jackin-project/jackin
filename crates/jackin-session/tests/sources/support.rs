// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{EnvSnapshot, FileMode, FileSystem, HostPath, ProcessRunner};
use jackin_session::{OnePasswordCli, SourceDesk};
use jackin_testkit::{MemFs, ProcessReply, ScriptedProcessRunner, ScriptedProgram, SeenCall};
use serde_json::{Value, json};

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) const CANARY: &str = "CANARY-op-secret-91c7d2e8";
pub(crate) const ACCOUNT_ID: &str = "3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f";
const GUARD: Duration = Duration::from_secs(10);

pub(crate) async fn guard(work: impl Future<Output = TestResult>) -> TestResult {
    tokio::time::timeout(GUARD, work)
        .await
        .map_err(|_| "the test hung for 10 s")?
}

pub(crate) fn host(path: &str) -> Result<HostPath, Box<dyn Error>> {
    Ok(HostPath::parse(std::path::Path::new(path))?)
}

pub(crate) struct Bench {
    pub(crate) fs: Arc<MemFs>,
    pub(crate) runner: Arc<ScriptedProcessRunner>,
    env: Vec<(String, String)>,
}

impl Bench {
    pub(crate) fn new() -> Bench {
        Bench {
            fs: Arc::new(MemFs::new()),
            runner: Arc::new(ScriptedProcessRunner::new()),
            env: Vec::new(),
        }
    }

    pub(crate) fn env(mut self, name: &str, value: &str) -> Bench {
        self.env.push((name.to_owned(), value.to_owned()));
        self
    }

    pub(crate) fn cli(&self) -> OnePasswordCli {
        let runner: Arc<dyn ProcessRunner> = Arc::<ScriptedProcessRunner>::clone(&self.runner);
        OnePasswordCli::new(runner)
    }

    pub(crate) fn desk(&self) -> SourceDesk {
        let fs: Arc<dyn FileSystem> = Arc::<MemFs>::clone(&self.fs);
        SourceDesk::new(fs, EnvSnapshot::from_pairs(self.env.clone()), self.cli())
    }

    pub(crate) fn op(&self, args: &[&str], reply: ProcessReply) {
        self.runner.expect(ScriptedProgram::Op, args, reply);
    }

    pub(crate) fn op_calls(&self) -> Vec<SeenCall> {
        self.runner.seen()
    }

    pub(crate) async fn login(&self, home: &HostPath, text: &str) -> TestResult {
        self.fs.create_dir_all(home).await?;
        let file = HostPath::parse(&home.as_path().join("auth.json"))?;
        self.fs
            .write_new(&file, text.as_bytes(), FileMode(0o600))
            .await?;
        Ok(())
    }
}

pub(crate) fn exited(stdout: &Value) -> ProcessReply {
    ProcessReply::exited(0, stdout.to_string().as_bytes(), b"")
}

pub(crate) fn failed(stderr: &str) -> ProcessReply {
    ProcessReply::exited(1, b"", stderr.as_bytes())
}

pub(crate) fn chatgpt_login() -> Value {
    json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "access_token": "ACCESS-FAKE-1",
            "refresh_token": "REFRESH-FAKE-1",
            "oauth_provenance": {"client_id": "app_fake"},
            "account_id": ACCOUNT_ID,
        },
    })
}

pub(crate) fn item(fields: &Value) -> Value {
    json!({
        "id": "item-1",
        "title": "OpenAI",
        "category": "API_CREDENTIAL",
        "fields": fields,
    })
}

pub(crate) fn concealed(id: &str, label: &str, value: &str) -> Value {
    json!({"id": id, "label": label, "type": "CONCEALED", "value": value})
}
