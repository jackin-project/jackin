// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::fixtures::FixtureError;
use crate::sync::locked;

const CHUNK: usize = 8192;
const DEVICE_ATTRIBUTES_QUERY: &[u8] = b"\x1b[c";
const DEVICE_ATTRIBUTES_REPLY: &[u8] = b"\x1b[?62c";
const POLL: Duration = Duration::from_millis(10);
const POLLS: usize = 2000;
const TAIL: usize = 600;

#[derive(Clone, Debug)]
pub struct PtyCommand {
    program: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cwd: Option<PathBuf>,
    size: (u16, u16),
}

impl PtyCommand {
    pub fn new(program: impl Into<PathBuf>) -> PtyCommand {
        PtyCommand {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            size: (100, 30),
        }
    }

    #[must_use]
    pub fn args<S: AsRef<str>>(mut self, args: &[S]) -> PtyCommand {
        self.args = args.iter().map(|arg| arg.as_ref().to_owned()).collect();
        self
    }

    #[must_use]
    pub fn env(mut self, name: &str, value: &str) -> PtyCommand {
        self.env.push((name.to_owned(), value.to_owned()));
        self
    }

    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> PtyCommand {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn spawn(self) -> Result<PtyRun, FixtureError> {
        let pair = native_pty_system().openpty(PtySize {
            rows: self.size.1,
            cols: self.size.0,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut command = CommandBuilder::new(&self.program);
        command.args(&self.args);
        command.env_clear();
        for (name, value) in &self.env {
            command.env(name, value);
        }
        if let Some(cwd) = &self.cwd {
            command.cwd(cwd);
        }
        let child = pair.slave.spawn_command(command)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let writer: Writer = Arc::new(Mutex::new(pair.master.take_writer()?));
        let output = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&output);
        let answers = Arc::clone(&writer);
        std::thread::Builder::new()
            .name("pty-reader".to_owned())
            .spawn(move || {
                let mut buffer = vec![0_u8; CHUNK];
                while let Ok(count) = reader.read(&mut buffer) {
                    if count == 0 {
                        break;
                    }
                    let chunk = &buffer[..count];
                    locked(&sink).extend_from_slice(chunk);
                    if chunk
                        .windows(DEVICE_ATTRIBUTES_QUERY.len())
                        .any(|window| window == DEVICE_ATTRIBUTES_QUERY)
                    {
                        let mut writer = locked(&answers);
                        drop(writer.write_all(DEVICE_ATTRIBUTES_REPLY));
                        drop(writer.flush());
                    }
                }
            })?;
        Ok(PtyRun {
            _master: pair.master,
            writer,
            child,
            output,
        })
    }
}

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

pub struct PtyRun {
    _master: Box<dyn MasterPty + Send>,
    writer: Writer,
    child: Box<dyn Child + Send + Sync>,
    output: Arc<Mutex<Vec<u8>>>,
}

impl fmt::Debug for PtyRun {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PtyRun").finish_non_exhaustive()
    }
}

impl PtyRun {
    pub fn output(&self) -> String {
        String::from_utf8_lossy(&locked(&self.output)).into_owned()
    }

    pub fn send(&mut self, bytes: &[u8]) -> Result<(), FixtureError> {
        let mut writer = locked(&self.writer);
        writer.write_all(bytes)?;
        writer.flush()?;
        Ok(())
    }

    pub async fn wait_for(&self, needle: &str) -> Result<String, FixtureError> {
        for _ in 0..POLLS {
            let output = self.output();
            if output.contains(needle) {
                return Ok(output);
            }
            tokio::time::sleep(POLL).await;
        }
        Err(format!(
            "{needle:?} never appeared; the output ended with:\n{}",
            self.tail()
        )
        .into())
    }

    pub async fn exit_code(&mut self) -> Result<u32, FixtureError> {
        for _ in 0..POLLS {
            if let Some(status) = self.child.try_wait()? {
                return Ok(status.exit_code());
            }
            tokio::time::sleep(POLL).await;
        }
        Err(format!(
            "the process never exited; the output ended with:\n{}",
            self.tail()
        )
        .into())
    }

    fn tail(&self) -> String {
        let output = self.output();
        let start = output.len().saturating_sub(TAIL);
        output
            .char_indices()
            .map(|(at, _)| at)
            .find(|at| *at >= start)
            .map_or_else(String::new, |at| output[at..].escape_debug().to_string())
    }
}

impl Drop for PtyRun {
    fn drop(&mut self) {
        drop(self.child.kill());
        drop(self.child.wait());
    }
}
