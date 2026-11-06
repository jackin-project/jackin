// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use russh::keys::PrivateKey;
use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::server::{self, Auth, Handle, Session};
use russh::{Channel, ChannelId, Sig};
use tokio::io::DuplexStream;

pub(crate) const RIGHT_SEED: [u8; 32] = [0x2a; 32];
pub(crate) const WRONG_SEED: [u8; 32] = [0x55; 32];
pub(crate) const RIGHT_FINGERPRINT: &str = "SHA256:1Lt0B6m58n9i5yp9hABit7qhCJ/7aR7kRxt8QRziO8k";
pub(crate) const WRONG_FINGERPRINT: &str = "SHA256:G0rarR9Wy+cqC2YbqeAoaVmUU9hm1OPuwLi2DueJAYU";
pub(crate) const SUBSYSTEM: &str = "openshell-main";
pub(crate) const READ_ONLY_ENV: &str = "OPENSHELL_MAIN_READ_ONLY";
pub(crate) const READ_ONLY_NOTICE: &str = "openshell: canonical main process already has an input owner; attached read-only; retry input after the owner disconnects; press Ctrl-C or Ctrl-D to exit\r\n";
pub(crate) const INPUT_ENABLED_NOTICE: &str = "openshell: input enabled\r\n";
pub(crate) const REFUSED_NOTICE: &str =
    "openshell: cannot connect to the canonical main process: no main session is available\r\n";

pub(crate) fn host_key(seed: &[u8; 32]) -> PrivateKey {
    PrivateKey::from(Ed25519Keypair::from_seed(seed))
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Data(Vec<u8>),
    Notice(&'static str),
    ExitStatus(u32),
    ExitSignal,
    Eof,
    Close,
}

#[derive(Clone, Debug)]
pub(crate) struct Reaction {
    pub(crate) when: Vec<u8>,
    pub(crate) then: Vec<Action>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Script {
    pub(crate) stall_subsystem: bool,
    pub(crate) after_subsystem: Vec<Action>,
    pub(crate) reactions: Vec<Reaction>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Log {
    pub(crate) auth_users: Vec<String>,
    pub(crate) channels_opened: usize,
    pub(crate) steps: Vec<String>,
    pub(crate) windows: Vec<(u32, u32, Instant)>,
    pub(crate) data: Vec<u8>,
    pub(crate) eof: bool,
    pub(crate) closed: bool,
}

struct Fired(Vec<bool>);

pub(crate) struct SshModel {
    key: PrivateKey,
    advertised: String,
    script: Script,
    log: Mutex<Log>,
    fired: Mutex<Fired>,
    live: tokio::sync::Mutex<Option<(Handle, ChannelId)>>,
    created: Mutex<Vec<String>>,
    revoked: Mutex<Vec<String>>,
}

impl std::fmt::Debug for SshModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshModel").finish_non_exhaustive()
    }
}

impl SshModel {
    pub(crate) fn new(presented: &PrivateKey, advertised: &str, script: Script) -> Arc<SshModel> {
        Arc::new(SshModel {
            key: presented.clone(),
            advertised: advertised.to_owned(),
            fired: Mutex::new(Fired(vec![false; script.reactions.len()])),
            script,
            log: Mutex::new(Log::default()),
            live: tokio::sync::Mutex::new(None),
            created: Mutex::new(Vec::new()),
            revoked: Mutex::new(Vec::new()),
        })
    }

    pub(crate) fn advertised(&self) -> &str {
        &self.advertised
    }

    pub(crate) fn log(&self) -> Log {
        self.log.lock().unwrap().clone()
    }

    pub(crate) fn note_created(&self, token: &str) {
        self.created.lock().unwrap().push(token.to_owned());
    }

    pub(crate) fn note_revoked(&self, token: &str) {
        self.revoked.lock().unwrap().push(token.to_owned());
    }

    pub(crate) fn created(&self) -> Vec<String> {
        self.created.lock().unwrap().clone()
    }

    pub(crate) fn revoked(&self) -> Vec<String> {
        self.revoked.lock().unwrap().clone()
    }

    pub(crate) fn serve(self: &Arc<SshModel>, stream: DuplexStream) {
        let config = Arc::new(server::Config {
            keys: vec![self.key.clone()],
            inactivity_timeout: Some(Duration::from_secs(60)),
            ..server::Config::default()
        });
        let handler = ModelHandler {
            model: Arc::clone(self),
        };
        tokio::spawn(async move {
            drop(server::run_stream(config, stream, handler).await);
        });
    }

    pub(crate) async fn push(&self, actions: &[Action]) {
        let live = self.live.lock().await.clone();
        let (handle, channel) = live.expect("no SSH channel is open");
        for action in actions {
            match action {
                Action::Data(bytes) => drop(handle.data(channel, bytes.clone()).await),
                Action::Notice(text) => {
                    drop(
                        handle
                            .extended_data(channel, 1, text.as_bytes().to_vec())
                            .await,
                    );
                }
                Action::ExitStatus(code) => drop(handle.exit_status_request(channel, *code).await),
                Action::ExitSignal => drop(
                    handle
                        .exit_signal_request(
                            channel,
                            Sig::TERM,
                            false,
                            String::new(),
                            String::new(),
                        )
                        .await,
                ),
                Action::Eof => drop(handle.eof(channel).await),
                Action::Close => drop(handle.close(channel).await),
            }
        }
    }
}

fn apply(session: &mut Session, channel: ChannelId, action: &Action) -> Result<(), russh::Error> {
    match action {
        Action::Data(bytes) => session.data(channel, bytes.clone()),
        Action::Notice(text) => session.extended_data(channel, 1, text.as_bytes().to_vec()),
        Action::ExitStatus(code) => session.exit_status_request(channel, *code),
        Action::ExitSignal => session.exit_signal_request(channel, Sig::TERM, false, "", ""),
        Action::Eof => session.eof(channel),
        Action::Close => session.close(channel),
    }
}

struct ModelHandler {
    model: Arc<SshModel>,
}

impl ModelHandler {
    fn step(&self, text: String) {
        self.model.log.lock().unwrap().steps.push(text);
    }

    fn answer_subsystem(
        &self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), russh::Error> {
        if self.model.script.stall_subsystem {
            return Ok(());
        }
        session.channel_success(channel)?;
        for action in &self.model.script.after_subsystem {
            apply(session, channel, action)?;
        }
        Ok(())
    }

    fn react(
        &self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), russh::Error> {
        let seen = {
            let mut log = self.model.log.lock().unwrap();
            log.data.extend_from_slice(data);
            log.data.clone()
        };
        for (index, reaction) in self.model.script.reactions.iter().enumerate() {
            let matches = seen
                .windows(reaction.when.len())
                .any(|window| window == reaction.when.as_slice());
            let first =
                matches && !std::mem::replace(&mut self.model.fired.lock().unwrap().0[index], true);
            if first {
                for action in &reaction.then {
                    apply(session, channel, action)?;
                }
            }
        }
        Ok(())
    }
}

impl server::Handler for ModelHandler {
    type Error = russh::Error;

    fn auth_none(&mut self, user: &str) -> impl Future<Output = Result<Auth, Self::Error>> + Send {
        self.model
            .log
            .lock()
            .unwrap()
            .auth_users
            .push(user.to_owned());
        std::future::ready(Ok(Auth::Accept))
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.model.log.lock().unwrap().channels_opened += 1;
        *self.model.live.lock().await = Some((session.handle(), channel.id()));
        reply.accept().await;
        Ok(())
    }

    fn env_request(
        &mut self,
        channel: ChannelId,
        variable_name: &str,
        variable_value: &str,
        session: &mut Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.step(format!("env {variable_name}={variable_value}"));
        std::future::ready(session.channel_success(channel))
    }

    fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.step(format!("pty {col_width}x{row_height}"));
        std::future::ready(session.channel_success(channel))
    }

    fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.step(format!("subsystem {name}"));
        std::future::ready(self.answer_subsystem(channel, session))
    }

    fn window_change_request(
        &mut self,
        channel: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        session: &mut Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.model
            .log
            .lock()
            .unwrap()
            .windows
            .push((col_width, row_height, Instant::now()));
        std::future::ready(session.channel_success(channel))
    }

    fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        std::future::ready(self.react(channel, data, session))
    }

    fn channel_eof(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.model.log.lock().unwrap().eof = true;
        std::future::ready(Ok(()))
    }

    fn channel_close(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.model.log.lock().unwrap().closed = true;
        std::future::ready(Ok(()))
    }
}
