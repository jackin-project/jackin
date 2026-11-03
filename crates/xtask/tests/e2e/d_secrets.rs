// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use jackin_xtask::{A1Error, A1Lock, A1Login, Redactor, fixture_login, host_login_state};
use regex::Regex;
use serde_json::{Value, json};

use super::scenario::world::{ACCESS, ACCOUNT_ID, CLIENT_ID, ID_TOKEN, REFRESH, auth_json};
use super::scenario::{Ran, Scenario};

fn needles() -> Vec<String> {
    let mut all = vec![ACCESS.to_owned(), REFRESH.to_owned(), ID_TOKEN.to_owned()];
    for token in [ACCESS, ID_TOKEN] {
        all.extend(
            token
                .split('.')
                .filter(|segment| segment.len() >= 16)
                .map(str::to_owned),
        );
    }
    all
}

fn assert_clean(case: &str, ran: &Ran, scenario: &Scenario) {
    let mut haystacks = vec![
        ("stdout".to_owned(), ran.stdout.clone()),
        ("stderr".to_owned(), ran.stderr.clone()),
    ];
    if let Ok(entries) = fs::read_dir(&scenario.world.log_dir) {
        for entry in entries.flatten() {
            haystacks.push((
                entry.path().display().to_string(),
                fs::read_to_string(entry.path()).unwrap_or_default(),
            ));
        }
    }
    for (name, text) in &haystacks {
        for needle in needles() {
            assert!(
                !text.contains(&needle),
                "{case}: {name} carries {:?}",
                &needle[..12]
            );
        }
    }
}

#[tokio::test]
async fn no_output_channel_carries_a_secret() {
    let plain = Scenario::new().await;
    let ran = plain.run().await;
    ran.expect("no leaks", &[]);
    assert!(
        ran.stdout.contains("7 checks pass") && ran.stdout.contains("second build cached"),
        "ordinary text survives"
    );
    assert_clean("baseline", &ran, &plain);

    let leaky = Scenario::new().await;
    {
        let mut knobs = leaky.knobs();
        knobs.leak_in_load_stderr = Some(ACCESS.to_owned());
        knobs.leak_in_pty = Some(REFRESH.to_owned());
    }
    let segment = ID_TOKEN.split('.').next().expect("segment");
    leaky.patch("J10", None, "/checks/0/result", json!("FAIL"));
    leaky.patch("J10", None, "/checks/0/detail", json!(segment));
    leaky.patch("J10", None, "/result", json!("FAIL"));
    leaky.patch("J10", None, "/reason", json!(format!("quoted {segment}")));
    let ran = leaky.run().await;
    assert_eq!(ran.status_of("J08"), "FAIL", "{}", ran.stdout);
    assert_eq!(ran.status_of("J10"), "FAIL", "{}", ran.stdout);
    let reason = ran.reason_of("J08");
    assert!(
        Regex::new(r"needle found in .* at byte \d+")
            .expect("regex")
            .is_match(&reason),
        "{reason}"
    );
    assert!(
        reason.contains("attach transcript"),
        "a token split across two PTY reads is still found: {reason}"
    );
    assert_clean("leaks", &ran, &leaky);

    let panics = Scenario::new().await;
    panics.knobs().panic_in_probe = Some(format!("boom {ACCESS}"));
    let ran = panics.run().await;
    assert_eq!(ran.status_of("J04"), "FAIL", "{}", ran.stdout);
    assert!(ran.reason_of("J04").contains("panicked"), "{}", ran.stdout);
    assert_clean("panic", &ran, &panics);

    let invalid = Scenario::new().await;
    invalid.patch("J04", None, "/facts/user", json!(ACCESS));
    invalid.patch("J04", None, "/facts/uid", json!(REFRESH));
    let ran = invalid.run().await;
    assert_eq!(ran.status_of("J04"), "FAIL", "{}", ran.stdout);
    assert_clean("schema violation", &ran, &invalid);
}

#[test]
fn short_strings_are_not_needles() {
    let short = "eyJhbGciOiJSUzI";
    let long = "eyJhbGciOiJSUzI1";
    assert_eq!(short.len(), 15);
    assert_eq!(long.len(), 16);
    let redactor = Redactor::new([short, long, "Bearer"]);
    assert_eq!(redactor.scrub("a Bearer b"), "a Bearer b");
    assert_eq!(
        redactor.scrub(&format!("x {short} y")),
        format!("x {short} y")
    );
    assert!(!redactor.scrub(&format!("x {long} y")).contains(long));
    let jwt =
        Redactor::new([format!("{short}.AAAAAAAAAAAAAAAAAAAA.BBBBBBBBBBBBBBBBBBBB").as_str()]);
    assert_eq!(
        jwt.scrub(short),
        short,
        "the short header part alone survives"
    );
    assert!(!jwt.scrub("AAAAAAAAAAAAAAAAAAAA").contains("AAAA"));
}

fn codex_home(dir: &Path, text: &str) {
    fs::create_dir_all(dir).expect("mkdir");
    fs::write(dir.join("auth.json"), text).expect("write");
}

#[test]
fn a1_login_never_prints_through_debug_or_error_text() {
    let dir = tempfile::tempdir().expect("tempdir");
    codex_home(
        dir.path(),
        &auth_json(ACCESS, REFRESH, ID_TOKEN, ACCOUNT_ID),
    );
    let login = A1Login::read(dir.path()).expect("valid login");
    for text in [format!("{login:?}"), format!("{login:#?}")] {
        for needle in needles() {
            assert!(!text.contains(&needle), "Debug leaks a token");
        }
    }
    let cases: Vec<(&str, Option<String>)> = vec![
        ("missing", None),
        (
            "api key",
            Some(json!({"auth_mode": "apikey", "OPENAI_API_KEY": ACCESS}).to_string()),
        ),
        (
            "tokens absent",
            Some(json!({"auth_mode": "chatgpt"}).to_string()),
        ),
        (
            "empty refresh",
            Some(auth_json(ACCESS, "", ID_TOKEN, ACCOUNT_ID)),
        ),
        (
            "malformed",
            Some(format!("{{ \"tokens\": {ACCESS} nonsense")),
        ),
    ];
    for (case, content) in cases {
        let home = dir.path().join(case.replace(' ', "-"));
        if let Some(text) = content {
            codex_home(&home, &text);
        }
        let error: A1Error = A1Login::read(&home).expect_err(case);
        let text = format!("{error} {error:?}");
        for needle in needles() {
            assert!(
                !text.contains(&needle),
                "{case}: the error carries file contents"
            );
        }
        assert!(
            text.contains("A1 has no usable source") || matches!(error, A1Error::Missing(_)),
            "{case}: {text}"
        );
    }
}

#[tokio::test]
async fn only_a_chatgpt_login_is_an_a1_source() {
    let dir = tempfile::tempdir().expect("tempdir");
    codex_home(
        dir.path(),
        &auth_json(ACCESS, REFRESH, ID_TOKEN, ACCOUNT_ID),
    );
    A1Login::read(dir.path()).unwrap();
    for (case, text) in [
        (
            "api key only",
            json!({"OPENAI_API_KEY": "sk-test-0123456789abcdef"}).to_string(),
        ),
        (
            "api key mode with tokens present",
            auth_json(ACCESS, REFRESH, ID_TOKEN, ACCOUNT_ID).replace("\"chatgpt\"", "\"apikey\""),
        ),
        (
            "empty refresh token",
            auth_json(ACCESS, "", ID_TOKEN, ACCOUNT_ID),
        ),
    ] {
        let scenario = Scenario::new().await;
        codex_home(&scenario.world.codex_home, &text);
        let ran = scenario.run().await;
        assert_eq!(ran.status.code(), 2, "{case}: {}", ran.stderr);
        assert!(
            ran.stderr.contains("A1 has no usable source"),
            "{case}: {}",
            ran.stderr
        );
        assert!(ran.stdout.is_empty(), "{case}: nothing runs");
    }
}

#[test]
fn fixture_logins_share_nothing_with_the_a1_login() {
    let root = tempfile::tempdir().expect("tempdir");
    let mut refresh: Vec<String> = Vec::new();
    let mut accounts: Vec<String> = Vec::new();
    for index in 0..10 {
        let dir = root.path().join(format!("fx{index}"));
        fixture_login(&dir, CLIENT_ID).expect("fixture");
        let text = fs::read_to_string(dir.join("auth.json")).expect("read");
        for token in [ACCESS, REFRESH, ID_TOKEN] {
            for window in token.as_bytes().windows(12) {
                assert!(
                    !text.as_bytes().windows(12).any(|part| part == window),
                    "fixture {index} shares bytes with an A1 token"
                );
            }
        }
        let value: Value = serde_json::from_str(&text).expect("json");
        assert_eq!(value["auth_mode"], "chatgpt");
        for key in ["access_token", "refresh_token", "account_id"] {
            assert!(
                value["tokens"][key]
                    .as_str()
                    .is_some_and(|text| !text.is_empty()),
                "tokens.{key}"
            );
        }
        assert_eq!(value["tokens"]["oauth_provenance"]["client_id"], CLIENT_ID);
        refresh.push(
            value["tokens"]["refresh_token"]
                .as_str()
                .expect("refresh")
                .to_owned(),
        );
        accounts.push(
            value["tokens"]["account_id"]
                .as_str()
                .expect("account")
                .to_owned(),
        );
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(dir.join("auth.json"))
                .expect("meta")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    refresh.sort();
    refresh.dedup();
    accounts.sort();
    accounts.dedup();
    assert_eq!(refresh.len(), 10, "refresh tokens differ");
    assert_eq!(accounts.len(), 10, "account ids differ");
    assert!(!accounts.iter().any(|id| id == ACCOUNT_ID));
}

#[test]
fn a1_lock_is_exclusive_and_fails_fast() {
    let dir = tempfile::tempdir().expect("tempdir");
    let first = A1Lock::acquire(dir.path()).expect("first");
    let started = Instant::now();
    let second = A1Lock::acquire(dir.path());
    assert!(matches!(second, Err(A1Error::LockHeld(_))), "{second:?}");
    assert!(
        started.elapsed().as_millis() < 500,
        "the second acquire must not wait"
    );
    drop(first);
    assert!(
        A1Lock::acquire(dir.path()).is_ok(),
        "released with the first handle"
    );
}

#[tokio::test]
async fn host_login_change_is_reported_without_a_hash() {
    let dir = tempfile::tempdir().expect("tempdir");
    codex_home(
        dir.path(),
        &auth_json(ACCESS, REFRESH, ID_TOKEN, ACCOUNT_ID),
    );
    let login = A1Login::read(dir.path()).expect("login");
    assert_eq!(host_login_state(dir.path(), &login), "unchanged");
    codex_home(
        dir.path(),
        &auth_json(ACCESS, REFRESH, ID_TOKEN, ACCOUNT_ID),
    );
    assert_eq!(
        host_login_state(dir.path(), &login),
        "unchanged",
        "rewritten with the same bytes"
    );
    let file = dir.path().join("auth.json");
    let copy = dir.path().join("reference.json");
    assert!(
        Command::new("cp")
            .arg("-p")
            .arg(&file)
            .arg(&copy)
            .status()
            .expect("cp")
            .success()
    );
    fs::write(
        &file,
        auth_json(
            ACCESS,
            "rt_changed0123456789abcdefghijklmnopq",
            ID_TOKEN,
            ACCOUNT_ID,
        ),
    )
    .expect("write");
    assert!(
        Command::new("touch")
            .arg("-r")
            .arg(&copy)
            .arg(&file)
            .status()
            .expect("touch")
            .success()
    );
    assert_eq!(
        host_login_state(dir.path(), &login),
        "CHANGED",
        "the old mtime does not hide a change"
    );

    let plain = Scenario::new().await;
    let ran = plain.run().await;
    assert!(
        ran.stdout.contains("a1_host_login=unchanged"),
        "{}",
        ran.stdout
    );
    let rotated = Scenario::new().await;
    rotated.knobs().rewrite_codex_home = true;
    let ran = rotated.run().await;
    let result = ran.stdout.lines().last().expect("result");
    assert!(result.ends_with("a1_host_login=CHANGED"), "{result}");
    assert!(
        !Regex::new("[0-9a-f]{32}").expect("regex").is_match(result),
        "no digest: {result}"
    );
}
