// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
mod case_06;
mod case_07;
mod case_08;
mod case_09;
mod case_10;
mod case_11;
mod case_12;
mod case_13;

use std::path::Path;

use super::Multiplexer;

pub(super) fn test_config(workdir: &Path) -> jackin_protocol::CapsuleConfig {
    jackin_protocol::CapsuleConfig {
        role: "test".to_owned(),
        workdir: workdir.display().to_string(),
        ..jackin_protocol::CapsuleConfig::default()
    }
}

pub(super) fn test_mux(workdir: &Path) -> Multiplexer {
    Multiplexer::new(24, 80, test_config(workdir)).unwrap()
}

pub(super) fn spawn_sleep_session(mux: &mut Multiplexer) -> u64 {
    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut cmd = portable_pty::CommandBuilder::new("/bin/sleep");
    cmd.arg("300");
    let terminal = mux.session_terminal(20, 60);
    let identity = jackin_protocol::SessionIdentity {
        uid: 65_534,
        gid: 65_534,
    };
    let (session, id) = crate::session::Session::spawn(
        crate::session::SessionSpawnSpec {
            label: "sleep".to_owned(),
            agent: None,
            account_id: None,
            identity,
            provider: None,
            cache_dir: None,
        },
        cmd,
        terminal,
        event_tx,
    )
    .unwrap();
    mux.session_supervisor.sessions.insert(id, session);
    id
}

pub(super) fn terminate_session(mux: &mut Multiplexer, id: u64) {
    if let Some(session) = mux.session_supervisor.sessions.remove(id) {
        session.terminate();
    }
}
