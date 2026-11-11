// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU16;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jackin_openshell::contract::NUDGE_CASES;
use jackin_openshell::{
    EndReason, Gateway, GatewayError, GrpcGateway, InputLease, InputMode, RepaintPolicy, TermSize,
    TerminalEvent, TerminalKind, TerminalRequest, TerminalSession,
};
use openshell_sdk::raw::proto::SandboxPhase;

use super::harness::{Env, WORKSPACE, connect, env, guard};
use super::ssh_server::{
    Action, INPUT_ENABLED_NOTICE, READ_ONLY_ENV, READ_ONLY_NOTICE, REFUSED_NOTICE,
    RIGHT_FINGERPRINT, RIGHT_SEED, Reaction, SUBSYSTEM, Script, SshModel, WRONG_FINGERPRINT,
    WRONG_SEED, host_key,
};
use super::support::{SANDBOX, add_sandbox, sandbox_name};

const QUIET: Duration = Duration::from_millis(200);
const EVENTUALLY: Duration = Duration::from_secs(5);

struct Rig {
    _env: Env,
    gateway: GrpcGateway,
    model: Arc<SshModel>,
}

async fn rig(seed: &[u8; 32], advertised: &str, script: Script) -> Rig {
    let env = env().await;
    add_sandbox(
        &mut env.server.state(),
        WORKSPACE,
        SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
    let model = SshModel::new(&host_key(seed), advertised, script);
    env.server.state().ssh = Some(Arc::clone(&model));
    let gateway = connect(&env, WORKSPACE).await;
    Rig {
        _env: env,
        gateway,
        model,
    }
}

async fn good(script: Script) -> Rig {
    rig(&RIGHT_SEED, RIGHT_FINGERPRINT, script).await
}

fn size(cols: u16, rows: u16) -> TermSize {
    TermSize {
        cols: NonZeroU16::new(cols).unwrap(),
        rows: NonZeroU16::new(rows).unwrap(),
    }
}

fn request(input: InputMode, repaint: RepaintPolicy, size: TermSize) -> TerminalRequest {
    TerminalRequest {
        sandbox: sandbox_name(SANDBOX),
        kind: TerminalKind::MainProcess { input, repaint },
        size,
    }
}

impl Rig {
    async fn try_open(
        &self,
        input: InputMode,
        repaint: RepaintPolicy,
        size: TermSize,
    ) -> Result<TerminalSession, GatewayError> {
        guard(self.gateway.open_terminal(&request(input, repaint, size))).await
    }

    async fn open(&self, input: InputMode) -> TerminalSession {
        self.try_open(input, RepaintPolicy::Never, size(100, 30))
            .await
            .unwrap()
    }

    fn windows(&self) -> Vec<(u32, u32)> {
        self.model
            .log()
            .windows
            .iter()
            .map(|window| (window.0, window.1))
            .collect()
    }
}

async fn take(session: &mut TerminalSession, count: usize) -> Vec<TerminalEvent> {
    let mut events = Vec::new();
    while events.len() < count {
        events.push(
            guard(session.next_event())
                .await
                .expect("the session ended"),
        );
    }
    events
}

async fn drain(session: &mut TerminalSession) -> Vec<TerminalEvent> {
    let mut events = Vec::new();
    while let Some(event) = guard(session.next_event()).await {
        events.push(event);
    }
    events
}

async fn quiet(session: &mut TerminalSession) {
    let waited = tokio::time::timeout(QUIET, session.next_event()).await;
    assert!(waited.is_err(), "unexpected event {waited:?}");
}

async fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + EVENTUALLY;
    while !condition() {
        assert!(Instant::now() < deadline, "{what}");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn then_end(status: u32) -> Vec<Action> {
    vec![Action::ExitStatus(status), Action::Eof, Action::Close]
}

fn reaction(when: &[u8], then: Vec<Action>) -> Reaction {
    Reaction {
        when: when.to_vec(),
        then,
    }
}

fn read_only() -> Script {
    Script {
        after_subsystem: vec![Action::Notice(READ_ONLY_NOTICE)],
        ..Script::default()
    }
}

fn notice(text: &str) -> TerminalEvent {
    TerminalEvent::Notice(text.trim().to_owned())
}

#[tokio::test]
async fn the_host_key_pin_accepts_the_advertised_key_and_refuses_every_other() {
    let script = Script {
        after_subsystem: vec![Action::Data(b"hello".to_vec())],
        ..Script::default()
    };
    let rig_ok = good(script).await;
    let mut session = rig_ok.open(InputMode::Owner).await;
    assert_eq!(
        take(&mut session, 3).await,
        vec![
            TerminalEvent::Attached,
            TerminalEvent::Lease(InputLease::Owner),
            TerminalEvent::Output(b"hello".to_vec()),
        ]
    );
    let log = rig_ok.model.log();
    assert_eq!(log.auth_users, ["sandbox"]);
    assert_eq!(log.steps, ["pty 100x30", &format!("subsystem {SUBSYSTEM}")]);

    for (seed, advertised) in [
        (&RIGHT_SEED, WRONG_FINGERPRINT),
        (&WRONG_SEED, RIGHT_FINGERPRINT),
        (&RIGHT_SEED, ""),
    ] {
        let refused = rig(seed, advertised, Script::default()).await;
        let error = refused
            .try_open(InputMode::Owner, RepaintPolicy::Never, size(100, 30))
            .await
            .unwrap_err();
        assert!(matches!(error, GatewayError::HostKeyMismatch), "{error:?}");
        tokio::time::sleep(Duration::from_millis(150)).await;
        let log = refused.model.log();
        assert!(log.auth_users.is_empty() && log.steps.is_empty() && log.data.is_empty());
        assert_eq!(log.channels_opened, 0);
        assert_eq!(refused.model.created().len(), 1);
        assert_eq!(refused.model.revoked(), refused.model.created());
    }
}

#[tokio::test]
async fn the_lease_follows_the_supervisor_notices_and_the_input_mode() {
    let owner = good(Script::default()).await;
    let mut session = owner.open(InputMode::Owner).await;
    assert_eq!(
        take(&mut session, 2).await,
        vec![
            TerminalEvent::Attached,
            TerminalEvent::Lease(InputLease::Owner)
        ]
    );
    quiet(&mut session).await;
    assert!(
        !owner
            .model
            .log()
            .steps
            .iter()
            .any(|step| step.starts_with("env "))
    );

    let waiting = good(read_only()).await;
    let mut session = waiting.open(InputMode::Owner).await;
    assert_eq!(
        take(&mut session, 3).await,
        vec![
            TerminalEvent::Attached,
            TerminalEvent::Lease(InputLease::ReadOnly),
            notice(READ_ONLY_NOTICE),
        ]
    );
    waiting
        .model
        .push(&[Action::Notice(READ_ONLY_NOTICE)])
        .await;
    assert_eq!(take(&mut session, 1).await, vec![notice(READ_ONLY_NOTICE)]);
    quiet(&mut session).await;
    waiting
        .model
        .push(&[Action::Notice(INPUT_ENABLED_NOTICE)])
        .await;
    assert_eq!(
        take(&mut session, 2).await,
        vec![
            TerminalEvent::Lease(InputLease::Owner),
            notice(INPUT_ENABLED_NOTICE),
        ]
    );
    waiting
        .model
        .push(&[Action::Notice(INPUT_ENABLED_NOTICE)])
        .await;
    assert_eq!(
        take(&mut session, 1).await,
        vec![notice(INPUT_ENABLED_NOTICE)]
    );
    quiet(&mut session).await;

    let refused = good(Script {
        after_subsystem: [vec![Action::Notice(REFUSED_NOTICE)], then_end(1)].concat(),
        ..Script::default()
    })
    .await;
    let mut session = refused.open(InputMode::Owner).await;
    assert_eq!(
        drain(&mut session).await,
        vec![
            TerminalEvent::Attached,
            TerminalEvent::Ended(EndReason::Refused(REFUSED_NOTICE.trim().to_owned())),
        ]
    );

    let viewer = good(Script::default()).await;
    let mut session = viewer.open(InputMode::Viewer).await;
    assert_eq!(
        take(&mut session, 2).await,
        vec![
            TerminalEvent::Attached,
            TerminalEvent::Lease(InputLease::ReadOnly)
        ]
    );
    assert_eq!(
        viewer.model.log().steps,
        [
            format!("env {READ_ONLY_ENV}=1"),
            "pty 100x30".to_owned(),
            format!("subsystem {SUBSYSTEM}"),
        ]
    );
}

#[tokio::test]
async fn the_detach_keys_end_the_session_without_an_exit_status() {
    let detach_script = || Script {
        reactions: vec![
            reaction(&[0x04], then_end(0)),
            reaction(&[0x10, 0x11], then_end(0)),
        ],
        ..Script::default()
    };
    let cases: [(&str, Vec<Vec<u8>>); 3] = [
        ("ctrl-d", vec![vec![0x04]]),
        ("ctrl-p ctrl-q in one chunk", vec![vec![0x10, 0x11]]),
        (
            "ctrl-p and ctrl-q in two chunks",
            vec![vec![0x10], vec![0x11]],
        ),
    ];
    for (label, chunks) in cases {
        let attached = good(detach_script()).await;
        let mut session = attached.open(InputMode::Owner).await;
        for chunk in chunks {
            session.input(chunk).await.unwrap();
        }
        assert_eq!(
            drain(&mut session).await,
            vec![
                TerminalEvent::Attached,
                TerminalEvent::Lease(InputLease::Owner),
                TerminalEvent::Ended(EndReason::Detached),
            ],
            "{label}"
        );
    }

    let not_a_detach = [
        ("ctrl-p then x", vec![vec![0x10], b"x".to_vec()], &b"x"[..]),
        (
            "ctrl-p x ctrl-q",
            vec![vec![0x10], b"x".to_vec(), vec![0x11]],
            &[0x11][..],
        ),
        ("ctrl-c under an owner lease", vec![vec![0x03]], &[0x03][..]),
    ];
    for (label, chunks, trigger) in not_a_detach {
        let attached = good(Script {
            reactions: vec![reaction(trigger, then_end(0))],
            ..Script::default()
        })
        .await;
        let mut session = attached.open(InputMode::Owner).await;
        let sent: Vec<u8> = chunks.concat();
        for chunk in chunks {
            session.input(chunk).await.unwrap();
        }
        assert_eq!(
            drain(&mut session).await,
            vec![
                TerminalEvent::Attached,
                TerminalEvent::Lease(InputLease::Owner),
                TerminalEvent::Exited(Some(0)),
                TerminalEvent::Ended(EndReason::StreamClosed),
            ],
            "{label}"
        );
        assert_eq!(attached.model.log().data, sent, "{label}");
    }
}

#[tokio::test]
async fn exit_statuses_and_ctrl_c_under_a_read_only_lease_are_reported_as_sent() {
    let mut script = read_only();
    script.reactions = vec![reaction(&[0x03], then_end(0))];
    let waiting = good(script).await;
    let mut session = waiting.open(InputMode::Owner).await;
    session.input(vec![0x03]).await.unwrap();
    assert_eq!(
        drain(&mut session).await,
        vec![
            TerminalEvent::Attached,
            TerminalEvent::Lease(InputLease::ReadOnly),
            notice(READ_ONLY_NOTICE),
            TerminalEvent::Ended(EndReason::StreamClosed),
        ]
    );

    let cases = [
        (
            "status 7",
            vec![Action::ExitStatus(7), Action::Eof, Action::Close],
            Some(7),
        ),
        ("status 0 with no key pending", then_end(0), Some(0)),
        (
            "a signal",
            vec![Action::ExitSignal, Action::Eof, Action::Close],
            None,
        ),
    ];
    for (label, then, exit) in cases {
        let attached = good(Script {
            reactions: vec![reaction(b"go", then)],
            ..Script::default()
        })
        .await;
        let mut session = attached.open(InputMode::Owner).await;
        session.input(b"go".to_vec()).await.unwrap();
        assert_eq!(
            drain(&mut session).await,
            vec![
                TerminalEvent::Attached,
                TerminalEvent::Lease(InputLease::Owner),
                TerminalEvent::Exited(exit),
                TerminalEvent::Ended(EndReason::StreamClosed),
            ],
            "{label}"
        );
    }
}

#[tokio::test]
async fn the_repaint_nudge_resizes_one_row_less_and_back_after_the_delay() {
    let delay = Duration::from_millis(600);
    let nudged = good(Script::default()).await;
    let started = Instant::now();
    let _session = nudged
        .try_open(
            InputMode::Owner,
            RepaintPolicy::NudgeAfter(delay),
            size(100, 30),
        )
        .await
        .unwrap();
    eventually("the nudge never arrived", || nudged.windows().len() >= 2).await;
    assert_eq!(nudged.windows(), [(100, 29), (100, 30)]);
    assert!(
        nudged.model.log().windows[0].2 >= started + delay,
        "the nudge came early"
    );

    let resized = good(Script::default()).await;
    let session = resized
        .try_open(
            InputMode::Owner,
            RepaintPolicy::NudgeAfter(delay),
            size(100, 30),
        )
        .await
        .unwrap();
    session.resize(size(80, 20)).await.unwrap();
    eventually("the nudge never arrived", || resized.windows().len() >= 3).await;
    assert_eq!(resized.windows(), [(80, 20), (80, 19), (80, 20)]);

    let never = good(Script::default()).await;
    let session = never.open(InputMode::Owner).await;
    session.resize(size(90, 25)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(never.windows(), [(90, 25)]);

    let one_row = good(Script::default()).await;
    let _session = one_row
        .try_open(
            InputMode::Owner,
            RepaintPolicy::NudgeAfter(Duration::from_millis(100)),
            size(100, 1),
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(one_row.windows().is_empty());
}

async fn settled() {
    tokio::task::spawn_blocking(|| {
        let (_open, idle) = std::sync::mpsc::channel::<()>();
        assert_eq!(
            idle.recv_timeout(Duration::from_millis(30)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        );
    })
    .await
    .unwrap();
}

fn pairs(cases: &[(u16, u16)]) -> Vec<(u32, u32)> {
    cases
        .iter()
        .map(|&(cols, rows)| (u32::from(cols), u32::from(rows)))
        .collect()
}

async fn arrived(what: &str, model: &Arc<SshModel>, windows: usize) {
    let model = Arc::clone(model);
    let found = tokio::task::spawn_blocking(move || {
        let (_open, idle) = std::sync::mpsc::channel::<()>();
        let deadline = Instant::now() + EVENTUALLY;
        while model.log().windows.len() < windows {
            if Instant::now() >= deadline {
                return false;
            }
            assert_eq!(
                idle.recv_timeout(Duration::from_millis(5)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            );
        }
        true
    })
    .await
    .unwrap();
    assert!(
        found,
        "{what}: the nudge did not arrive when the delay had passed"
    );
}

#[tokio::test]
async fn the_repaint_nudge_follows_tokio_time_for_every_shared_case() {
    let delay = Duration::from_secs(2);
    for case in NUDGE_CASES {
        let rig = good(Script::default()).await;
        let policy = if case.nudge {
            RepaintPolicy::NudgeAfter(delay)
        } else {
            RepaintPolicy::Never
        };
        let opening = Instant::now();
        let session = rig
            .try_open(InputMode::Owner, policy, size(case.open.0, case.open.1))
            .await
            .unwrap();
        for &(cols, rows) in case.resize_before {
            session.resize(size(cols, rows)).await.unwrap();
        }
        let before = pairs(case.resize_before);
        eventually(case.name, || rig.windows().len() >= before.len()).await;

        tokio::time::pause();
        let quiet = delay.saturating_sub(opening.elapsed() + Duration::from_millis(20));
        settled().await;
        tokio::time::advance(quiet).await;
        settled().await;
        assert_eq!(rig.windows(), before, "{}: the nudge came early", case.name);
        tokio::time::advance(delay.saturating_sub(quiet) + Duration::from_millis(10)).await;
        arrived(case.name, &rig.model, case.windows.len()).await;
        assert_eq!(rig.windows(), pairs(case.windows), "{}", case.name);
        tokio::time::advance(delay * 3).await;
        settled().await;
        assert_eq!(
            rig.windows(),
            pairs(case.windows),
            "{}: the nudge fired twice",
            case.name
        );
        tokio::time::resume();
        drop(session);
    }
}

#[tokio::test]
async fn the_session_token_is_revoked_once_on_every_way_a_session_ends() {
    let ends = Script {
        reactions: vec![reaction(b"go", then_end(0))],
        ..Script::default()
    };
    for how in ["detach", "drop", "server close"] {
        let attached = good(ends.clone()).await;
        let mut session = attached.open(InputMode::Owner).await;
        assert!(
            attached.model.revoked().is_empty(),
            "{how}: revoked while live"
        );
        match how {
            "detach" => session.detach().await.unwrap(),
            "drop" => drop(session),
            _ => {
                session.input(b"go".to_vec()).await.unwrap();
                drain(&mut session).await;
            }
        }
        eventually(
            &format!("{how}: the session token was never revoked"),
            || !attached.model.revoked().is_empty(),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(attached.model.created().len(), 1, "{how}");
        assert_eq!(attached.model.revoked(), attached.model.created(), "{how}");
    }
}

#[tokio::test]
async fn dropping_the_open_future_during_the_handshake_revokes_the_token() {
    let stalled = good(Script {
        stall_subsystem: true,
        ..Script::default()
    })
    .await;
    let wanted = request(InputMode::Owner, RepaintPolicy::Never, size(100, 30));
    let mut open = Box::pin(stalled.gateway.open_terminal(&wanted));
    let step = format!("subsystem {SUBSYSTEM}");
    tokio::select! {
        opened = &mut open => panic!("the open finished: {:?}", opened.map(|_| ())),
        () = async {
            while !stalled.model.log().steps.contains(&step) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        } => {}
    }
    assert!(stalled.model.revoked().is_empty());
    drop(open);
    eventually("a dropped attach left its token unrevoked", || {
        !stalled.model.revoked().is_empty()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(stalled.model.created().len(), 1);
    assert_eq!(stalled.model.revoked(), stalled.model.created());
}

#[tokio::test]
async fn a_viewer_never_sends_input_but_still_ends_on_the_detach_keys() {
    let viewer = good(Script::default()).await;
    let mut session = viewer.open(InputMode::Viewer).await;
    take(&mut session, 2).await;
    session.input(b"ls\r".to_vec()).await.unwrap();
    session.resize(size(80, 20)).await.unwrap();
    eventually("the resize never arrived", || !viewer.windows().is_empty()).await;
    assert!(viewer.model.log().data.is_empty());

    let cases = [
        ("ctrl-d", vec![vec![0x04]], EndReason::Detached),
        (
            "ctrl-p ctrl-q",
            vec![vec![0x10], vec![0x11]],
            EndReason::Detached,
        ),
        ("ctrl-c", vec![vec![0x03]], EndReason::StreamClosed),
    ];
    for (label, chunks, reason) in cases {
        let attached = good(Script::default()).await;
        let mut session = attached.open(InputMode::Viewer).await;
        for chunk in chunks {
            session.input(chunk).await.unwrap();
        }
        assert_eq!(
            drain(&mut session).await,
            vec![
                TerminalEvent::Attached,
                TerminalEvent::Lease(InputLease::ReadOnly),
                TerminalEvent::Ended(reason),
            ],
            "{label}"
        );
        assert!(attached.model.log().data.is_empty(), "{label}");
    }

    let waiting = good(read_only()).await;
    let mut session = waiting.open(InputMode::Owner).await;
    take(&mut session, 3).await;
    session.input(b"x".to_vec()).await.unwrap();
    eventually("the owner input was dropped", || {
        waiting.model.log().data == b"x"
    })
    .await;
    waiting
        .model
        .push(&[Action::Notice(INPUT_ENABLED_NOTICE)])
        .await;
    assert_eq!(
        take(&mut session, 2).await,
        vec![
            TerminalEvent::Lease(InputLease::Owner),
            notice(INPUT_ENABLED_NOTICE),
        ]
    );
}

#[tokio::test]
async fn output_and_notices_keep_the_order_the_supervisor_sent_them() {
    let notice_first = good(Script {
        after_subsystem: vec![
            Action::Notice(READ_ONLY_NOTICE),
            Action::Data(b"out".to_vec()),
        ],
        ..Script::default()
    })
    .await;
    let mut session = notice_first.open(InputMode::Owner).await;
    assert_eq!(
        take(&mut session, 4).await,
        vec![
            TerminalEvent::Attached,
            TerminalEvent::Lease(InputLease::ReadOnly),
            notice(READ_ONLY_NOTICE),
            TerminalEvent::Output(b"out".to_vec()),
        ]
    );

    let data_first = good(Script {
        after_subsystem: vec![
            Action::Data(b"out".to_vec()),
            Action::Notice(READ_ONLY_NOTICE),
        ],
        ..Script::default()
    })
    .await;
    let mut session = data_first.open(InputMode::Owner).await;
    assert_eq!(
        take(&mut session, 5).await,
        vec![
            TerminalEvent::Attached,
            TerminalEvent::Lease(InputLease::Owner),
            TerminalEvent::Output(b"out".to_vec()),
            TerminalEvent::Lease(InputLease::ReadOnly),
            notice(READ_ONLY_NOTICE),
        ]
    );
}
