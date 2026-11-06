// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::{self, Debug};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{plain, plain_duration};
use jackin_session::ProcessGroup;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use thiserror::Error;
use tokio::sync::mpsc;

use super::redact::Redactor;

const CHUNK: usize = 8192;
const QUEUE: usize = 256;
const EXIT_POLL: Duration = Duration::from_millis(25);
const TAIL_CHARS: usize = 400;

pub(crate) const DETACH: u8 = 0x04;
pub(crate) const INTERRUPT: u8 = 0x03;

#[derive(Debug, Error)]
pub enum PtyError {
    #[error("cannot open the pseudo terminal: {}", plain(.0))]
    Open(String),
    #[error("pseudo terminal i/o failed: {}", plain(.0))]
    Io(String),
    #[error("timed out after {} waiting for {}; the screen ended with: {}", plain_duration(.after), plain(.what), plain(.tail))]
    Timeout {
        what: String,
        after: Duration,
        tail: String,
    },
    #[error("the terminal closed while waiting for {}; the screen ended with: {}", plain(.what), plain(.tail))]
    Closed { what: String, tail: String },
}

#[derive(Clone, Debug)]
pub struct PtySpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
    pub cols: u16,
    pub rows: u16,
}

pub trait PtyHost: Send + Sync + Debug {
    fn open(&self, spec: &PtySpec) -> Result<Box<dyn PtyLink>, PtyError>;
}

#[async_trait]
pub trait PtyLink: Send + Debug {
    async fn recv(&mut self) -> Option<Vec<u8>>;
    fn send(&mut self, bytes: &[u8]) -> Result<(), PtyError>;
    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), PtyError>;
    fn try_exit(&mut self) -> Option<i32>;
    fn kill(&mut self);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PortablePty {}

impl PtyHost for PortablePty {
    fn open(&self, spec: &PtySpec) -> Result<Box<dyn PtyLink>, PtyError> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: spec.rows,
                cols: spec.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| PtyError::Open(error.to_string()))?;
        let mut command = CommandBuilder::new(&spec.program);
        command.args(&spec.args);
        command.env_clear();
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        if let Some(cwd) = &spec.cwd {
            command.cwd(cwd);
        }
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| PtyError::Open(error.to_string()))?;
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| PtyError::Open(error.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| PtyError::Open(error.to_string()))?;
        let (sender, chunks) = mpsc::channel(QUEUE);
        std::thread::Builder::new()
            .name("e2e-pty-reader".to_owned())
            .spawn(move || {
                let mut buffer = vec![0_u8; CHUNK];
                while let Ok(count) = reader.read(&mut buffer) {
                    if count == 0 || sender.blocking_send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| PtyError::Open(error.to_string()))?;
        Ok(Box::new(PortableLink {
            master: pair.master,
            writer,
            child,
            chunks,
        }))
    }
}

struct PortableLink {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    chunks: mpsc::Receiver<Vec<u8>>,
}

impl Debug for PortableLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PortableLink")
            .field("pid", &self.child.process_id())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl PtyLink for PortableLink {
    async fn recv(&mut self) -> Option<Vec<u8>> {
        self.chunks.recv().await
    }

    fn send(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        self.writer
            .write_all(bytes)
            .and_then(|()| self.writer.flush())
            .map_err(|error| PtyError::Io(error.to_string()))
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), PtyError> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| PtyError::Io(error.to_string()))
    }

    fn try_exit(&mut self) -> Option<i32> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(i32::try_from(status.exit_code()).unwrap_or(i32::MAX)),
            Ok(None) | Err(_) => None,
        }
    }

    fn kill(&mut self) {
        if let Some(group) = self.child.process_id().and_then(ProcessGroup::of_leader) {
            drop(group.sweep());
        }
        drop(self.child.kill());
        drop(self.child.wait());
    }
}

impl Drop for PortableLink {
    fn drop(&mut self) {
        self.kill();
    }
}

#[derive(Debug, Default)]
pub(crate) struct AnsiStripper {
    pending: Vec<u8>,
    utf8: Vec<u8>,
}

fn escape_len(buf: &[u8]) -> Option<usize> {
    let kind = *buf.get(1)?;
    match kind {
        b'[' => buf[2..]
            .iter()
            .position(|byte| (0x40..=0x7e).contains(byte))
            .map(|at| at + 3),
        b']' | b'P' | b'X' | b'^' | b'_' => {
            for (at, byte) in buf.iter().enumerate().skip(2) {
                if *byte == 0x07 {
                    return Some(at + 1);
                }
                if *byte == 0x1b {
                    return buf.get(at + 1).map(|_| at + 2);
                }
            }
            None
        }
        b'(' | b')' | b'*' | b'+' => (buf.len() >= 3).then_some(3),
        _ => Some(2),
    }
}

impl AnsiStripper {
    pub(crate) fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut at = 0;
        while at < self.pending.len() {
            let byte = self.pending[at];
            if byte == 0x1b {
                match escape_len(&self.pending[at..]) {
                    Some(len) => at += len,
                    None => break,
                }
            } else {
                if byte == b'\n' || byte == b'\t' || (byte >= 0x20 && byte != 0x7f) {
                    self.utf8.push(byte);
                }
                at += 1;
            }
        }
        self.pending.drain(..at);
        let valid = match std::str::from_utf8(&self.utf8) {
            Ok(text) => text.len(),
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(_) => self.utf8.len(),
        };
        let text = String::from_utf8_lossy(&self.utf8[..valid]).into_owned();
        self.utf8.drain(..valid);
        text
    }
}

pub struct PtySession {
    link: Box<dyn PtyLink>,
    stripper: AnsiStripper,
    text: String,
    transcript: Vec<u8>,
    redactor: Redactor,
}

impl Debug for PtySession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PtySession")
            .field("transcript_len", &self.transcript.len())
            .finish_non_exhaustive()
    }
}

impl PtySession {
    #[must_use]
    pub fn new(link: Box<dyn PtyLink>, redactor: Redactor) -> PtySession {
        PtySession {
            link,
            stripper: AnsiStripper::default(),
            text: String::new(),
            transcript: Vec::new(),
            redactor,
        }
    }

    #[must_use]
    pub fn transcript(&self) -> &[u8] {
        &self.transcript
    }

    fn tail(&self) -> String {
        let chars: Vec<char> = self.text.chars().collect();
        let start = chars.len().saturating_sub(TAIL_CHARS);
        self.redactor
            .scrub(&chars[start..].iter().collect::<String>())
    }

    fn absorb(&mut self, chunk: &[u8]) {
        self.transcript.extend_from_slice(chunk);
        let clean = self.stripper.push(chunk);
        self.text.push_str(&clean);
    }

    pub fn send(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        self.link.send(bytes)
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), PtyError> {
        self.link.resize(cols, rows)
    }

    pub async fn expect_with(
        &mut self,
        what: &str,
        timeout: Duration,
        find: impl Fn(&str) -> Option<(usize, usize)>,
    ) -> Result<(String, String), PtyError> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some((start, end)) = find(&self.text) {
                let before = self.text[..start].to_owned();
                let matched = self.text[start..end].to_owned();
                self.text.drain(..end);
                return Ok((before, matched));
            }
            match tokio::time::timeout_at(deadline, self.link.recv()).await {
                Ok(Some(chunk)) => self.absorb(&chunk),
                Ok(None) => {
                    return Err(PtyError::Closed {
                        what: what.to_owned(),
                        tail: self.tail(),
                    });
                }
                Err(_) => {
                    return Err(PtyError::Timeout {
                        what: what.to_owned(),
                        after: timeout,
                        tail: self.tail(),
                    });
                }
            }
        }
    }

    pub async fn expect(&mut self, pattern: &str, timeout: Duration) -> Result<String, PtyError> {
        self.expect_with(pattern, timeout, |text| {
            text.find(pattern)
                .map(|start| (start, start + pattern.len()))
        })
        .await
        .map(|(before, _)| before)
    }

    pub async fn wait_exit(&mut self, timeout: Duration) -> Result<i32, PtyError> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some(code) = self.link.try_exit() {
                while let Ok(Some(chunk)) =
                    tokio::time::timeout(Duration::from_millis(50), self.link.recv()).await
                {
                    self.absorb(&chunk);
                }
                return Ok(code);
            }
            tokio::select! {
                chunk = self.link.recv() => {
                    if let Some(chunk) = chunk {
                        self.absorb(&chunk);
                    } else {
                        tokio::time::sleep(EXIT_POLL).await;
                    }
                }
                () = tokio::time::sleep(EXIT_POLL) => {}
                () = tokio::time::sleep_until(deadline) => {
                    return Err(PtyError::Timeout {
                        what: "the child to exit".to_owned(),
                        after: timeout,
                        tail: self.tail(),
                    });
                }
            }
        }
    }

    pub async fn run_shell(
        &mut self,
        command: &str,
        timeout: Duration,
    ) -> Result<(String, i32), PtyError> {
        let mut entropy = [0_u8; 4];
        getrandom::fill(&mut entropy).map_err(|error| PtyError::Io(error.to_string()))?;
        let nonce = 100_000 + u64::from(u32::from_le_bytes(entropy)) % 800_000;
        let (begin, end) = (nonce, nonce + 1);
        self.send(
            format!(
                "printf 'B%sB\\n' $(({}+7)); ( {command} ); printf 'E%s-%sE\\n' $(({}+7)) $?\n",
                begin - 7,
                end - 7
            )
            .as_bytes(),
        )?;
        let start_marker = format!("B{begin}B");
        self.expect(&start_marker, timeout).await?;
        let end_prefix = format!("E{end}-");
        let (body, matched) = self
            .expect_with("the end marker", timeout, |text| {
                let start = text.find(&end_prefix)?;
                let rest = &text[start + end_prefix.len()..];
                let digits = rest.chars().take_while(char::is_ascii_digit).count();
                (digits > 0 && rest[digits..].starts_with('E'))
                    .then(|| (start, start + end_prefix.len() + digits + 1))
            })
            .await?;
        let code = matched
            .trim_start_matches(&end_prefix)
            .trim_end_matches('E')
            .parse()
            .map_err(|_| PtyError::Io("unreadable exit marker".to_owned()))?;
        Ok((body.trim_start_matches('\n').to_owned(), code))
    }
}
