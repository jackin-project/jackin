// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    DirEntry, EnvValue, FileIdentity, FileKind, FileLock, FileMode, FileSystem, FsError, HostPath,
    LineSink, LinkProblem, ProcessError, ProcessOutput, ProcessRunner, ProcessSpec, ProcessStatus,
    ProcessStdin, Program, RegularFile, RegularFileProblem, RepoPath, SecretOutput,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Default)]
pub(crate) struct RegistryFs {
    files: Mutex<BTreeMap<HostPath, Vec<u8>>>,
    reads: Mutex<Vec<HostPath>>,
    mutations: Mutex<Vec<String>>,
    fail_with: Mutex<Option<FsError>>,
}

impl RegistryFs {
    pub(crate) fn put(&self, path: &HostPath, bytes: &[u8]) {
        self.files
            .lock()
            .unwrap()
            .insert(path.clone(), bytes.to_vec());
    }

    pub(crate) fn remove(&self, path: &HostPath) {
        self.files.lock().unwrap().remove(path);
    }

    pub(crate) fn reads(&self) -> Vec<HostPath> {
        self.reads.lock().unwrap().clone()
    }

    pub(crate) fn mutations(&self) -> Vec<String> {
        self.mutations.lock().unwrap().clone()
    }

    pub(crate) fn fail_next_read(&self, error: FsError) {
        *self.fail_with.lock().unwrap() = Some(error);
    }

    fn record(&self, entry: String) {
        self.mutations.lock().unwrap().push(entry);
    }
}

#[async_trait]
impl FileSystem for RegistryFs {
    async fn read(&self, path: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        self.reads.lock().unwrap().push(path.clone());
        if let Some(error) = self.fail_with.lock().unwrap().take() {
            return Err(error);
        }
        Ok(self.files.lock().unwrap().get(path).cloned())
    }

    async fn stat(&self, path: &HostPath) -> Result<Option<FileKind>, FsError> {
        Ok(self
            .files
            .lock()
            .unwrap()
            .contains_key(path)
            .then_some(FileKind::File))
    }

    async fn stat_following(&self, path: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.stat(path).await
    }
    async fn canonicalize(&self, path: &HostPath) -> Result<Option<HostPath>, FsError> {
        Ok(self.stat_following(path).await?.map(|_| path.clone()))
    }

    async fn identity(&self, _path: &HostPath) -> Result<Option<FileIdentity>, FsError> {
        Ok(None)
    }

    async fn file_len(&self, path: &HostPath) -> Result<Option<u64>, FsError> {
        Ok(self
            .files
            .lock()
            .unwrap()
            .get(path)
            .map(|bytes| bytes.len() as u64))
    }

    async fn list(&self, _dir: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        Ok(Vec::new())
    }

    async fn create_dir_all(&self, dir: &HostPath) -> Result<(), FsError> {
        self.record(format!("create_dir_all {dir:?}"));
        Ok(())
    }

    async fn create_private_dir_all(&self, dir: &HostPath) -> Result<(), FsError> {
        self.record(format!("create_private_dir_all {dir:?}"));
        Ok(())
    }

    async fn write_new(
        &self,
        path: &HostPath,
        _bytes: &[u8],
        _mode: FileMode,
    ) -> Result<(), FsError> {
        self.record(format!("write_new {path:?}"));
        Ok(())
    }

    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), FsError> {
        self.record(format!("rename {from:?} {to:?}"));
        Ok(())
    }

    async fn remove_file(&self, path: &HostPath) -> Result<(), FsError> {
        self.record(format!("remove_file {path:?}"));
        Ok(())
    }

    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), FsError> {
        self.record(format!("remove_dir_all {path:?}"));
        Ok(())
    }

    async fn mode(&self, _path: &HostPath) -> Result<Option<FileMode>, FsError> {
        Ok(None)
    }

    async fn sync_dir(&self, dir: &HostPath) -> Result<(), FsError> {
        self.record(format!("sync_dir {dir:?}"));
        Ok(())
    }

    async fn read_at(
        &self,
        path: &HostPath,
        _offset: u64,
        _len: usize,
    ) -> Result<Vec<u8>, FsError> {
        Err(FsError::Io {
            path: path.clone(),
            kind: std::io::ErrorKind::Unsupported,
            message: "read_at is unsupported by RegistryFs".to_owned(),
        })
    }

    async fn read_link(
        &self,
        root: &HostPath,
        _file: &RepoPath,
    ) -> Result<Result<String, LinkProblem>, FsError> {
        Err(FsError::Io {
            path: root.clone(),
            kind: std::io::ErrorKind::Unsupported,
            message: "read_link is unsupported by RegistryFs".to_owned(),
        })
    }

    async fn write_symlink(&self, path: &HostPath, _target: &str) -> Result<(), FsError> {
        Err(FsError::Io {
            path: path.clone(),
            kind: std::io::ErrorKind::Unsupported,
            message: "write_symlink is unsupported by RegistryFs".to_owned(),
        })
    }

    async fn read_regular(
        &self,
        root: &HostPath,
        file: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        let path =
            HostPath::parse(&root.as_path().join(file.as_str())).map_err(|error| FsError::Io {
                path: root.clone(),
                kind: std::io::ErrorKind::InvalidInput,
                message: error.to_string(),
            })?;
        self.reads.lock().unwrap().push(path.clone());
        if let Some(error) = self.fail_with.lock().unwrap().take() {
            return Err(error);
        }
        Ok(self
            .files
            .lock()
            .unwrap()
            .get(&path)
            .map(|bytes| RegularFile {
                bytes: bytes
                    .iter()
                    .take(max_bytes.saturating_add(1))
                    .copied()
                    .collect(),
                mode: FileMode(0o600),
            })
            .ok_or(RegularFileProblem::Missing))
    }

    async fn lock(&self, path: &HostPath) -> Result<FileLock, FsError> {
        self.record(format!("lock {path:?}"));
        Err(FsError::LockHeld(path.clone()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Recorded {
    pub(crate) program: Program,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: Option<HostPath>,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) env_remove_prefixes: Vec<String>,
    pub(crate) stdin_null: bool,
    pub(crate) has_sink: bool,
    pub(crate) timeout: Duration,
}

impl Recorded {
    fn removes(&self, name: &str) -> bool {
        self.env_remove_prefixes
            .iter()
            .any(|prefix| name.starts_with(prefix.as_str()))
    }

    pub(crate) fn assert_git_env_isolated(&self) {
        self.assert_openshell_env_isolated();
        for inherited in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"] {
            assert!(self.removes(inherited), "{inherited} is inherited");
        }
    }

    pub(crate) fn assert_openshell_env_isolated(&self) {
        assert!(
            self.env
                .iter()
                .all(|(name, _)| !name.starts_with("OPENSHELL_")),
            "{:?}",
            self.env
        );
        for inherited in [
            "OPENSHELL_GATEWAY_ENDPOINT",
            "OPENSHELL_GATEWAY_INSECURE",
            "OPENSHELL_GATEWAY",
            "OPENSHELL_WORKSPACE",
            "OPENSHELL_FUTURE_SETTING",
        ] {
            assert!(self.removes(inherited), "{inherited} is inherited");
        }
        for kept in ["PATH", "HOME", "HTTPS_PROXY"] {
            assert!(!self.removes(kept), "{kept} is removed");
        }
    }
}

pub(crate) type Reply = Arc<dyn Fn(&Recorded) -> Result<ProcessOutput, ProcessError> + Send + Sync>;

#[derive(Clone)]
pub(crate) struct ScriptedRunner {
    reply: Reply,
    calls: Arc<Mutex<Vec<Recorded>>>,
}

impl std::fmt::Debug for ScriptedRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptedRunner").finish_non_exhaustive()
    }
}

pub(crate) fn exited(code: i32, stdout: &[u8], stderr: &[u8]) -> ProcessOutput {
    ProcessOutput {
        status: ProcessStatus::Exited(code),
        stdout: stdout.to_vec(),
        stderr: stderr.to_vec(),
    }
}

impl ScriptedRunner {
    pub(crate) fn new(
        reply: impl Fn(&Recorded) -> Result<ProcessOutput, ProcessError> + Send + Sync + 'static,
    ) -> ScriptedRunner {
        ScriptedRunner {
            reply: Arc::new(reply),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(crate) fn always(output: ProcessOutput) -> ScriptedRunner {
        ScriptedRunner::new(move |_| Ok(output.clone()))
    }

    pub(crate) fn calls(&self) -> Vec<Recorded> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl ProcessRunner for ScriptedRunner {
    async fn run(
        &self,
        spec: &ProcessSpec,
        lines: Option<&LineSink>,
        _cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        let recorded = Recorded {
            program: spec.program,
            args: spec.args.clone(),
            cwd: spec.cwd.clone(),
            env: spec
                .env
                .iter()
                .map(|(name, value)| {
                    let text = match value {
                        EnvValue::Plain(text) => text.clone(),
                        EnvValue::Secret(_) => "<secret>".to_owned(),
                    };
                    (name.clone(), text)
                })
                .collect(),
            env_remove_prefixes: spec.env_remove_prefixes.clone(),
            stdin_null: matches!(spec.stdin, ProcessStdin::Null),
            has_sink: lines.is_some(),
            timeout: spec.timeout,
        };
        match spec.program {
            Program::Openshell => {
                self.calls.lock().unwrap().push(recorded.clone());
                (self.reply)(&recorded)
            }
            other => Err(ProcessError::Spawn {
                program: other,
                message: "only openshell is available".to_owned(),
            }),
        }
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        _cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        Err(ProcessError::Spawn {
            program: spec.program,
            message: "secret runs are not scripted".to_owned(),
        })
    }
}
