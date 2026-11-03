// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use jackin_xtask::{PortablePty, PtyError, PtyHost, PtySession, PtySpec, Redactor};
use rustix::process::{Pid, test_kill_process};

use super::scenario::world::ACCESS;

const WAIT: Duration = Duration::from_secs(2);

fn spec(program: &str, args: &[&str]) -> PtySpec {
    let mut env = BTreeMap::new();
    env.insert("PATH".to_owned(), "/usr/bin:/bin".to_owned());
    env.insert("PS1".to_owned(), String::new());
    env.insert("PS2".to_owned(), String::new());
    env.insert("ENV".to_owned(), String::new());
    PtySpec {
        program: program.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        cwd: None,
        env,
        cols: 100,
        rows: 30,
    }
}

fn session(script: &str, redactor: Redactor) -> PtySession {
    let link = PortablePty::default()
        .open(&spec("/bin/sh", &["-c", script]))
        .expect("open");
    PtySession::new(link, redactor)
}

#[tokio::test]
async fn expect_returns_text_before_the_match_and_keeps_the_rest() {
    let mut screen = session(
        "printf 'abc'; sleep 0.3; printf 'def'; sleep 0.3; printf 'ghi'; sleep 1",
        Redactor::none(),
    );
    assert_eq!(screen.expect("cdefg", WAIT).await.expect("match"), "ab");
    assert_eq!(
        screen.expect("hi", WAIT).await.expect("rest"),
        "",
        "the remainder stayed in the buffer"
    );

    let mut quiet = session(
        &format!("printf 'visible {ACCESS}'; sleep 3"),
        Redactor::new([ACCESS]),
    );
    let error = quiet
        .expect("never appears", Duration::from_millis(600))
        .await
        .expect_err("timeout");
    match error {
        PtyError::Timeout { tail, .. } => {
            assert!(tail.contains("visible"), "{tail}");
            assert!(!tail.contains(ACCESS), "the tail is redacted: {tail}");
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn ansi_sequences_do_not_defeat_matching() {
    let header = "OpenAI Codex (v0.166.0)";
    for (case, script) in [
        (
            "csi colors",
            "printf '\\033[31mOpenAI \\033[0mCodex (v0.166.0)'",
        ),
        (
            "cursor moves",
            "printf '\\033[2;3HOpenAI Codex\\033[1A (v0.166.0)'",
        ),
        (
            "osc with bel",
            "printf '\\033]0;title\\007OpenAI Codex (v0.166.0)'",
        ),
        (
            "osc with st",
            "printf '\\033]0;title\\033\\\\OpenAI Codex (v0.166.0)'",
        ),
        (
            "dec private mode",
            "printf '\\033[?25lOpenAI Codex (v0.166.0)\\033[?25h'",
        ),
        (
            "split sequence",
            "printf '\\033[3'; sleep 0.3; printf '1mOpenAI Codex (v0.166.0)'",
        ),
    ] {
        let mut screen = session(&format!("{script}; sleep 1"), Redactor::none());
        screen
            .expect(header, WAIT)
            .await
            .unwrap_or_else(|error| panic!("{case}: {error}"));
    }
}

#[tokio::test]
async fn markers_defeat_the_terminal_echo() {
    let shell = || {
        let link = PortablePty::default()
            .open(&spec("/bin/sh", &["-i"]))
            .expect("open");
        PtySession::new(link, Redactor::none())
    };
    let mut screen = shell();
    assert_eq!(
        screen.run_shell("printf ok", WAIT).await.expect("run"),
        ("ok".to_owned(), 0)
    );
    assert_eq!(
        screen
            .run_shell("sh -c 'exit 3'", WAIT)
            .await
            .expect("exit")
            .1,
        3
    );
    let (body, code) = screen
        .run_shell("printf 'B42B\\nE43-0E\\n'", WAIT)
        .await
        .expect("markers in the output");
    assert_eq!(code, 0);
    assert!(
        body.contains("B42B") && body.contains("E43-0E"),
        "output that looks like a marker is data: {body:?}"
    );
    let echoed = screen
        .run_shell("printf 'second'", WAIT)
        .await
        .expect("again");
    assert_eq!(
        echoed.0, "second",
        "the echoed command line is not part of the answer"
    );
}

#[tokio::test]
async fn a_detach_key_ends_a_scripted_attach_child() {
    let script = "stty raw -echo; printf 'HEADER\\n'; b=$(dd bs=1 count=1 2>/dev/null | od -An -tx1 | tr -d ' \\n'); if [ \"$b\" = 04 ]; then exit 0; fi; exit 9";
    for round in 0..2 {
        let mut screen = session(script, Redactor::none());
        screen
            .expect("HEADER", WAIT)
            .await
            .unwrap_or_else(|error| panic!("round {round}: the replayed header: {error}"));
        screen.send(&[0x04]).expect("send");
        assert_eq!(
            screen.wait_exit(WAIT).await.expect("exit"),
            0,
            "round {round}"
        );
    }
    let mut silent = session(script, Redactor::none());
    silent.expect("HEADER", WAIT).await.expect("header");
    let error = silent
        .wait_exit(Duration::from_millis(400))
        .await
        .expect_err("no key, no exit");
    assert!(matches!(error, PtyError::Timeout { .. }), "{error:?}");
}

#[tokio::test]
async fn dropping_a_link_kills_the_child_and_its_group() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pidfile = dir.path().join("pid");
    let script = format!(
        "trap '' HUP; (trap '' HUP; exec sleep 600) & echo $! > {}; echo up; wait",
        pidfile.display()
    );
    let mut screen = session(&script, Redactor::none());
    screen.expect("up", WAIT).await.expect("up");
    let pid: i32 = std::fs::read_to_string(&pidfile)
        .expect("pid")
        .trim()
        .parse()
        .expect("number");
    let alive = |pid: i32| Pid::from_raw(pid).is_some_and(|pid| test_kill_process(pid).is_ok());
    assert!(alive(pid));
    drop(screen);
    let mut gone = false;
    for _ in 0..100 {
        if !alive(pid) {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(gone, "the grandchild is gone");
}

#[tokio::test]
async fn resize_reaches_the_child() {
    let script = "trap 'stty size' WINCH; echo ready; while :; do sleep 0.1; done";
    let mut screen = session(script, Redactor::none());
    screen.expect("ready", WAIT).await.expect("ready");
    screen.resize(77, 31).expect("resize");
    screen
        .expect("31 77", WAIT)
        .await
        .expect("the child sees the new size");
}
