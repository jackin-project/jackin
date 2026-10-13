// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn spawn_spec<'a>(
    agent: &'a str,
    instance: &'a str,
    auth_mode: Option<&'a str>,
    env_passthrough: &'a [(String, String)],
) -> AgentSpawnSpec<'a> {
    let (home_dir, forwarded_dir) = match agent {
        "claude" => (
            jackin_core::container_paths::CLAUDE_CONFIG_DIR,
            "/jackin/claude",
        ),
        "codex" => ("/home/agent/.codex", "/jackin/codex"),
        _ => ("/home/agent/.test", "/jackin/test"),
    };
    AgentSpawnSpec {
        agent,
        instance,
        home_dir,
        forwarded_dir,
        model: None,
        effort: None,
        auth_mode,
        env_passthrough,
        cwd: Path::new("/workspace"),
        codename: "test",
        identity: jackin_protocol::SessionIdentity {
            uid: 2_000,
            gid: 2_000,
        },
    }
}

#[derive(Debug)]
pub(super) struct NullChildKiller;

impl ChildKiller for NullChildKiller {
    fn kill(&mut self) -> std::io::Result<()> {
        Ok(())
    }
    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(Self)
    }
}

pub(super) struct NullMasterPty;

impl MasterPty for NullMasterPty {
    fn resize(&self, _size: PtySize) -> Result<()> {
        Ok(())
    }
    fn get_size(&self) -> Result<PtySize> {
        Ok(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
    }
    fn try_clone_reader(&self) -> Result<Box<dyn std::io::Read + Send>> {
        Ok(Box::new(std::io::empty()))
    }
    fn take_writer(&self) -> Result<Box<dyn std::io::Write + Send>> {
        Ok(Box::new(std::io::sink()))
    }
    #[cfg(unix)]
    fn process_group_leader(&self) -> Option<libc::pid_t> {
        None
    }
    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<portable_pty::unix::RawFd> {
        None
    }
    #[cfg(unix)]
    fn tty_name(&self) -> Option<std::path::PathBuf> {
        None
    }
}

pub(super) struct RecordingMasterPty {
    pub(super) inner: NullMasterPty,
    pub(super) last_size: Arc<Mutex<Option<PtySize>>>,
}

impl MasterPty for RecordingMasterPty {
    fn resize(&self, size: PtySize) -> Result<()> {
        if let Ok(mut slot) = self.last_size.lock() {
            *slot = Some(size);
        }
        Ok(())
    }
    fn get_size(&self) -> Result<PtySize> {
        self.inner.get_size()
    }
    fn try_clone_reader(&self) -> Result<Box<dyn std::io::Read + Send>> {
        self.inner.try_clone_reader()
    }
    fn take_writer(&self) -> Result<Box<dyn std::io::Write + Send>> {
        self.inner.take_writer()
    }
    #[cfg(unix)]
    fn process_group_leader(&self) -> Option<libc::pid_t> {
        self.inner.process_group_leader()
    }
    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<portable_pty::unix::RawFd> {
        self.inner.as_raw_fd()
    }
    #[cfg(unix)]
    fn tty_name(&self) -> Option<std::path::PathBuf> {
        self.inner.tty_name()
    }
}

pub(super) fn test_session_with_policy(policy: OscPolicy) -> Session {
    let (input_tx, _input_rx) = mpsc::unbounded_channel();
    let mut session = Session::new_for_test(
        "Test".to_owned(),
        Some("codex".to_owned()),
        None,
        (24, 80),
        100,
        input_tx,
        Arc::new(Mutex::new(Box::new(NullMasterPty))),
        Arc::new(Mutex::new(Box::new(NullChildKiller))),
    );
    session.osc_policy = policy;
    session
}

pub(super) fn test_process_info(pid: u32, agent: Agent) -> ProcessInfo {
    ProcessInfo {
        pid,
        pgid: pid,
        tpgid: i32::try_from(pid).unwrap(),
        cmdline: vec![agent.slug().to_owned()],
        exe_path: Some(std::path::PathBuf::from(format!(
            "/usr/local/bin/{}",
            agent.slug()
        ))),
        comm: agent.slug().to_owned(),
    }
}

#[derive(Debug)]
pub(super) struct StaticProcessSampler {
    physics_available: bool,
    root: Option<ProcessInfo>,
    foreground: ForegroundGroup,
    pub(super) descendants: u32,
    pub(super) cpu_delta: u64,
}

impl StaticProcessSampler {
    pub(super) fn foreground_agent(pid: u32, agent: Agent) -> Self {
        Self {
            physics_available: true,
            root: Some(test_process_info(pid, agent)),
            foreground: ForegroundGroup::Agent { agent, pgid: pid },
            descendants: 0,
            cpu_delta: 0,
        }
    }
}

impl ProcessSampler for StaticProcessSampler {
    fn physics_available(&self) -> bool {
        self.physics_available
    }

    fn read_process_info(&self, _pid: u32) -> Option<ProcessInfo> {
        self.root.clone()
    }

    fn foreground_group(&self, _root_info: &ProcessInfo) -> ForegroundGroup {
        self.foreground
    }

    fn descendant_process_count(&self, _root_pid: u32) -> u32 {
        self.descendants
    }

    fn sample_cpu_jiffies_delta(
        &mut self,
        _pid: u32,
        _previous: &mut Option<ProcessCpuSample>,
        _now: std::time::Instant,
    ) -> u64 {
        self.cpu_delta
    }
}

pub(super) fn status_test_registry() -> RulePackRegistry {
    let pack = toml::from_str::<RulePack>(
        "schema_version = 1\n\
         agent = \"codex\"\n\
         validated_versions = \">=1.0.0, <2.0.0\"\n\
         [[rule]]\n\
         id = \"blocked-test\"\n\
         state = \"blocked\"\n\
         priority = 100\n\
         strength = \"strong\"\n\
         region = \"bottom:24\"\n\
         requires_any = [\"approve?\"]\n\
         [[rule]]\n\
         id = \"idle-test\"\n\
         state = \"idle\"\n\
         priority = 90\n\
         strength = \"strong\"\n\
         region = \"bottom:24\"\n\
         requires_any = [\"ready\"]\n",
    )
    .unwrap()
    .finalize()
    .unwrap();
    RulePackRegistry::from_packs([pack])
}

pub(super) fn drained(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(bytes);
    session.drain_passthrough()
}

pub(super) fn drained_with_policy(bytes: &[u8], policy: OscPolicy) -> Vec<Vec<u8>> {
    let mut session = test_session_with_policy(policy);
    session.feed_pty(bytes);
    session.drain_passthrough()
}

pub(super) struct FaultMasterPty {
    pub(super) take_writer_err: Option<std::io::ErrorKind>,
    pub(super) clone_reader_err: Option<std::io::ErrorKind>,
    pub(super) writer_fails_after: Option<usize>,
    pub(super) reader_yields: Vec<Result<Vec<u8>, std::io::ErrorKind>>,
    pub(super) write_count: Arc<std::sync::atomic::AtomicUsize>,
    pub(super) reader_idx: Arc<std::sync::atomic::AtomicUsize>,
}

impl Default for FaultMasterPty {
    fn default() -> Self {
        Self {
            take_writer_err: None,
            clone_reader_err: None,
            writer_fails_after: None,
            reader_yields: Vec::new(),
            write_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            reader_idx: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

pub(super) struct FaultWriter {
    fails_after: Option<usize>,
    count: Arc<std::sync::atomic::AtomicUsize>,
}

impl std::io::Write for FaultWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        use std::sync::atomic::Ordering;
        let n = self.count.fetch_add(1, Ordering::SeqCst);
        if self.fails_after.is_some_and(|after| n >= after) {
            return Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) struct FaultReader {
    yields: Vec<Result<Vec<u8>, std::io::ErrorKind>>,
    idx: Arc<std::sync::atomic::AtomicUsize>,
}

impl std::io::Read for FaultReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use std::sync::atomic::Ordering;
        let i = self.idx.fetch_add(1, Ordering::SeqCst);
        if i >= self.yields.len() {
            return Ok(0);
        }
        match &self.yields[i] {
            Ok(data) => {
                let n = data.len().min(buf.len());
                buf[..n].copy_from_slice(&data[..n]);
                Ok(n)
            }
            Err(kind) => Err(std::io::Error::from(*kind)),
        }
    }
}

impl MasterPty for FaultMasterPty {
    fn resize(&self, _size: PtySize) -> Result<()> {
        Ok(())
    }
    fn get_size(&self) -> Result<PtySize> {
        Ok(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
    }
    fn try_clone_reader(&self) -> Result<Box<dyn std::io::Read + Send>> {
        if let Some(kind) = self.clone_reader_err {
            return Err(anyhow::anyhow!(std::io::Error::from(kind)));
        }
        Ok(Box::new(FaultReader {
            yields: self.reader_yields.clone(),
            idx: Arc::clone(&self.reader_idx),
        }))
    }
    fn take_writer(&self) -> Result<Box<dyn std::io::Write + Send>> {
        if let Some(kind) = self.take_writer_err {
            return Err(anyhow::anyhow!(std::io::Error::from(kind)));
        }
        Ok(Box::new(FaultWriter {
            fails_after: self.writer_fails_after,
            count: Arc::clone(&self.write_count),
        }))
    }
    #[cfg(unix)]
    fn process_group_leader(&self) -> Option<libc::pid_t> {
        None
    }
    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<portable_pty::unix::RawFd> {
        None
    }
    #[cfg(unix)]
    fn tty_name(&self) -> Option<std::path::PathBuf> {
        None
    }
}
