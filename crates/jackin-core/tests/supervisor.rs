// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{AttemptId, IdError, SupervisorState, SupervisorStateError};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn attempt(text: &str) -> Result<AttemptId, IdError> {
    AttemptId::parse(text)
}

#[test]
fn supervisor_state_parses_what_jackin_start_writes() -> TestResult {
    let cases = [
        ("waiting", SupervisorState::Waiting),
        (
            "running 1",
            SupervisorState::Running {
                attempt: attempt("1")?,
            },
        ),
        (
            "exited 1 3",
            SupervisorState::Exited {
                attempt: attempt("1")?,
                code: 3,
            },
        ),
        (
            "exited 12 0",
            SupervisorState::Exited {
                attempt: attempt("12")?,
                code: 0,
            },
        ),
        (
            "exited 7 255",
            SupervisorState::Exited {
                attempt: attempt("7")?,
                code: 255,
            },
        ),
        (
            "running 999999999999999999",
            SupervisorState::Running {
                attempt: attempt("999999999999999999")?,
            },
        ),
    ];
    for (text, state) in cases {
        for written in [text.to_owned(), format!("{text}\n")] {
            let parsed = SupervisorState::parse(&written)?;
            assert_eq!(parsed, state, "{written:?}");
            assert_eq!(parsed.render(), text);
            assert_eq!(SupervisorState::parse(&parsed.render())?, parsed);
        }
    }
    Ok(())
}

#[derive(Debug)]
enum Reason {
    Empty,
    TooLong(usize, usize),
    CharAt(char, usize),
    Format,
}

#[derive(Debug)]
enum Expect {
    Empty,
    UnknownWord(&'static str),
    BadAttempt(Reason),
    BadExitCode(&'static str),
    TrailingText(&'static str),
}

fn reason_matches(reason: &Reason, error: &IdError) -> bool {
    match (reason, error) {
        (Reason::Empty, IdError::Empty { .. }) | (Reason::Format, IdError::BadFormat { .. }) => {
            true
        }
        (Reason::TooLong(max, len), IdError::TooLong { max: m, len: l, .. }) => {
            max == m && len == l
        }
        (
            Reason::CharAt(ch, index),
            IdError::InvalidChar {
                ch: c, index: i, ..
            },
        ) => ch == c && index == i,
        _ => false,
    }
}

fn matches_expectation(expect: &Expect, error: &SupervisorStateError) -> bool {
    match (expect, error) {
        (Expect::Empty, SupervisorStateError::Empty) => true,
        (Expect::UnknownWord(want), SupervisorStateError::UnknownWord(got)) => want == got,
        (Expect::BadAttempt(reason), SupervisorStateError::BadAttempt(source)) => {
            reason_matches(reason, source)
        }
        (Expect::BadExitCode(want), SupervisorStateError::BadExitCode(got)) => want == got,
        (Expect::TrailingText(want), SupervisorStateError::TrailingText(got)) => want == got,
        _ => false,
    }
}

#[test]
fn supervisor_state_rejects_each_malformed_text_with_its_reason() -> TestResult {
    let cases = [
        ("", Expect::Empty),
        ("\n", Expect::Empty),
        ("paused", Expect::UnknownWord("paused")),
        ("WAITING", Expect::UnknownWord("WAITING")),
        ("Running 1", Expect::UnknownWord("Running")),
        ("running", Expect::BadAttempt(Reason::Empty)),
        ("running 0", Expect::BadAttempt(Reason::Format)),
        ("running 01", Expect::BadAttempt(Reason::Format)),
        ("running a1", Expect::BadAttempt(Reason::CharAt('a', 0))),
        (
            "running 1000000000000000000",
            Expect::BadAttempt(Reason::TooLong(18, 19)),
        ),
        ("exited 0 3", Expect::BadAttempt(Reason::Format)),
        ("exited 1", Expect::BadExitCode("")),
        ("exited 1 x", Expect::BadExitCode("x")),
        ("exited 1 99999999999", Expect::BadExitCode("99999999999")),
        ("exited 1 3 4", Expect::TrailingText("4")),
        ("waiting now", Expect::TrailingText("now")),
        ("running 1 2", Expect::TrailingText("2")),
    ];
    for (text, expect) in cases {
        let error = SupervisorState::parse(text)
            .err()
            .ok_or_else(|| format!("{text:?} was accepted"))?;
        assert!(
            matches_expectation(&expect, &error),
            "{text:?}: wanted {expect:?}, got {error:?}"
        );
    }
    for text in [
        "waiting\n\n",
        "waiting\r\n",
        " waiting",
        "waiting ",
        "running  1",
    ] {
        assert!(SupervisorState::parse(text).is_err(), "{text:?}");
    }
    Ok(())
}

#[test]
fn next_attempt_is_the_only_way_to_choose_a_start_request() -> TestResult {
    let max = attempt("999999999999999999")?;
    let waiting = SupervisorState::Waiting;
    assert_eq!(waiting.attempt(), None);
    assert_eq!(waiting.next_attempt(), Some(AttemptId::FIRST));
    let running = SupervisorState::Running {
        attempt: attempt("3")?,
    };
    assert_eq!(running.attempt(), Some(attempt("3")?));
    assert_eq!(running.next_attempt(), None);
    let exited = SupervisorState::Exited {
        attempt: attempt("3")?,
        code: 0,
    };
    assert_eq!(exited.attempt(), Some(attempt("3")?));
    assert_eq!(exited.next_attempt(), Some(attempt("4")?));
    let failed = SupervisorState::Exited {
        attempt: attempt("1")?,
        code: 7,
    };
    assert_eq!(failed.next_attempt(), Some(attempt("2")?));
    assert_eq!(
        SupervisorState::Exited {
            attempt: max,
            code: 0
        }
        .next_attempt(),
        None
    );
    assert_eq!(
        SupervisorState::Running { attempt: max }.next_attempt(),
        None
    );
    Ok(())
}

#[test]
fn supervisor_errors_show_the_words_read_from_the_sandbox_as_plain_text() {
    let shown = |text: &str| {
        SupervisorState::parse(text)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default()
    };
    assert_eq!(shown("pwn 1"), "unknown supervisor state word pwn");
    assert_eq!(
        shown("pwn\u{1b}[2J"),
        "unknown supervisor state word pwn\\u{1b}[2J"
    );
    assert_eq!(shown("exited 1 x"), "invalid exit code x");
    assert_eq!(shown("exited 1 x\u{1b}"), "invalid exit code x\\u{1b}");
    assert_eq!(shown("waiting now"), "trailing text now");
    assert_eq!(shown("waiting \u{1b}[2J"), "trailing text \\u{1b}[2J");
}
