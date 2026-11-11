// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use jackin_xtask::{
    A1_LOCK_FILE, A1Error, A1Lock, A1Login, Redactor, a1_lock_dir, fixture_login, host_login_state,
};
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

fn tampered(path: &[&str], replacement: Option<Value>) -> String {
    let mut document: Value =
        serde_json::from_str(&auth_json(ACCESS, REFRESH, ID_TOKEN, ACCOUNT_ID)).expect("json");
    let (last, parents) = path.split_last().expect("path");
    let mut node = &mut document;
    for key in parents {
        node = node.get_mut(*key).expect("parent");
    }
    let object = node.as_object_mut().expect("object");
    match replacement {
        Some(value) => {
            object.insert((*last).to_owned(), value);
        }
        None => {
            object.remove(*last).expect("present");
        }
    }
    document.to_string()
}

fn field_cases() -> Vec<(&'static str, &'static str, String)> {
    let empty = || Some(json!(""));
    vec![
        (
            "access token absent",
            "access_token",
            tampered(&["tokens", "access_token"], None),
        ),
        (
            "access token empty",
            "access_token",
            tampered(&["tokens", "access_token"], empty()),
        ),
        (
            "refresh token absent",
            "refresh_token",
            tampered(&["tokens", "refresh_token"], None),
        ),
        (
            "client id absent",
            "oauth_provenance.client_id",
            tampered(&["tokens", "oauth_provenance", "client_id"], None),
        ),
        (
            "client id empty",
            "oauth_provenance.client_id",
            tampered(&["tokens", "oauth_provenance", "client_id"], empty()),
        ),
        (
            "provenance absent",
            "oauth_provenance.client_id",
            tampered(&["tokens", "oauth_provenance"], None),
        ),
        (
            "account id absent",
            "account_id",
            tampered(&["tokens", "account_id"], None),
        ),
        (
            "account id empty",
            "account_id",
            tampered(&["tokens", "account_id"], empty()),
        ),
    ]
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
    let cases = cases.into_iter().chain(
        field_cases()
            .into_iter()
            .map(|(case, _, text)| (case, Some(text))),
    );
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
    for (case, field, text) in [
        (
            "api key only",
            json!({"OPENAI_API_KEY": REFRESH}).to_string(),
        ),
        (
            "api key mode with tokens present",
            auth_json(ACCESS, REFRESH, ID_TOKEN, ACCOUNT_ID).replace("\"chatgpt\"", "\"apikey\""),
        ),
        (
            "empty refresh token",
            auth_json(ACCESS, "", ID_TOKEN, ACCOUNT_ID),
        ),
    ]
    .into_iter()
    .map(|(case, text)| (case, None, text))
    .chain(
        field_cases()
            .into_iter()
            .map(|(case, field, text)| (case, Some(field), text)),
    ) {
        let scenario = Scenario::new().await;
        codex_home(&scenario.world.codex_home, &text);
        let ran = scenario.run().await;
        assert_eq!(ran.status.code(), 2, "{case}: {}", ran.stderr);
        assert!(
            ran.stderr.contains("A1 has no usable source"),
            "{case}: {}",
            ran.stderr
        );
        if let Some(field) = field {
            assert!(
                ran.stderr.contains(&format!("{field} is empty or absent")),
                "{case}: {}",
                ran.stderr
            );
        }
        for needle in needles() {
            assert!(!ran.stderr.contains(&needle), "{case}: stderr leaks");
        }
        assert!(ran.stdout.is_empty(), "{case}: nothing runs");
        assert_eq!(scenario.state().builds, 0, "{case}: nothing builds");
    }
}

#[test]
fn fixture_logins_share_nothing_with_the_a1_login() {
    let root = tempfile::tempdir().expect("tempdir");
    let mut collected: BTreeMap<&str, Vec<String>> = BTreeMap::new();
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
        let mut within: Vec<String> = Vec::new();
        for key in ["access_token", "refresh_token", "id_token", "account_id"] {
            let token = value["tokens"][key]
                .as_str()
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| panic!("tokens.{key} is empty or absent"));
            assert!(
                !within.iter().any(|earlier| earlier == token),
                "fixture {index} reuses one value for {key} and an earlier field"
            );
            within.push(token.to_owned());
            collected.entry(key).or_default().push(token.to_owned());
        }
        assert_eq!(value["tokens"]["oauth_provenance"]["client_id"], CLIENT_ID);
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
    for (key, a1) in [
        ("access_token", ACCESS),
        ("refresh_token", REFRESH),
        ("id_token", ID_TOKEN),
        ("account_id", ACCOUNT_ID),
    ] {
        let mut values = collected.remove(key).expect("collected");
        assert_eq!(values.len(), 10, "{key} collected");
        assert!(!values.iter().any(|value| value == a1), "{key} reuses A1");
        values.sort();
        values.dedup();
        assert_eq!(values.len(), 10, "{key} values differ across fixtures");
        collected.insert(key, values);
    }
    let mut every: Vec<&String> = collected.values().flatten().collect();
    let total = every.len();
    assert_eq!(total, 40);
    every.sort();
    every.dedup();
    assert_eq!(every.len(), total, "no value repeats across any field");
    for a1 in [ACCESS, REFRESH, ID_TOKEN, ACCOUNT_ID] {
        assert!(!every.iter().any(|value| value.as_str() == a1));
    }
}

#[tokio::test]
async fn setup_supplies_three_independent_logins_to_the_account_boundary() {
    let scenario = Scenario::new().await;
    let ran = scenario.run().await;
    assert_eq!(ran.status.code(), 0, "{} {}", ran.stdout, ran.stderr);
    let supplied = scenario.state().supplied_logins.clone();
    let names = ["e2e-abcd-a1", "e2e-abcd-fxr", "e2e-abcd-fxw"];
    let documents: Vec<Value> = names
        .iter()
        .map(|name| {
            let bytes = supplied
                .get(*name)
                .unwrap_or_else(|| panic!("no login was supplied for {name}"));
            serde_json::from_slice(bytes).unwrap_or_else(|_| panic!("{name} login is not JSON"))
        })
        .collect();
    assert_eq!(
        supplied["e2e-abcd-a1"],
        fs::read(scenario.world.codex_home.join("auth.json")).expect("a1 login"),
        "A1 supplies the host login as it is"
    );
    for first in 0..3 {
        for second in (first + 1)..3 {
            assert_ne!(
                supplied[names[first]], supplied[names[second]],
                "{} and {} supply one document",
                names[first], names[second]
            );
            assert_ne!(
                documents[first], documents[second],
                "{} and {} supply one login",
                names[first], names[second]
            );
        }
    }
    let mut seen: Vec<(String, String)> = Vec::new();
    for (index, document) in documents.iter().enumerate() {
        for key in ["access_token", "refresh_token", "id_token", "account_id"] {
            let value = document["tokens"][key]
                .as_str()
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| panic!("{} has no tokens.{key}", names[index]))
                .to_owned();
            assert!(
                !seen.iter().any(|(_, earlier)| *earlier == value),
                "{} tokens.{key} repeats a value of another account or field",
                names[index]
            );
            seen.push((format!("{}.{key}", names[index]), value));
        }
        assert_eq!(document["auth_mode"], "chatgpt");
        assert_eq!(
            document["tokens"]["oauth_provenance"]["client_id"],
            CLIENT_ID
        );
    }
    assert_eq!(seen.len(), 12);
}

#[test]
fn a1_lock_is_exclusive_and_fails_fast() {
    let dir = tempfile::tempdir().expect("tempdir");
    let first = A1Lock::acquire(dir.path()).expect("first");
    let started = Instant::now();
    let (sender, receiver) = std::sync::mpsc::channel();
    let path = dir.path().to_path_buf();
    std::thread::spawn(move || {
        drop(sender.send(A1Lock::acquire(&path).map(drop)));
    });
    let second = receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the second acquire must answer at once, not wait for the first");
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

#[test]
fn a1_lock_never_follows_or_truncates_what_it_finds_at_its_path() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let denied = |result: Result<A1Lock, A1Error>| {
        assert!(matches!(result, Err(A1Error::LockIo { .. })), "{result:?}");
    };

    let dir = tempfile::tempdir().expect("tempdir");
    let victim = dir.path().join("victim.txt");
    fs::write(&victim, b"keep me").expect("victim");
    let lock_dir = dir.path().join(a1_lock_dir());
    let held = A1Lock::acquire(dir.path()).expect("a fresh directory");
    assert_eq!(
        fs::metadata(&lock_dir).expect("dir").permissions().mode() & 0o777,
        0o700
    );
    drop(held);
    fs::remove_file(lock_dir.join(A1_LOCK_FILE)).expect("remove the lock file");
    symlink(&victim, lock_dir.join(A1_LOCK_FILE)).expect("link");
    denied(A1Lock::acquire(dir.path()));
    assert_eq!(fs::read(&victim).expect("victim"), b"keep me");

    let elsewhere = tempfile::tempdir().expect("tempdir");
    let other = tempfile::tempdir().expect("tempdir");
    fs::set_permissions(elsewhere.path(), fs::Permissions::from_mode(0o700)).expect("chmod");
    symlink(elsewhere.path(), other.path().join(a1_lock_dir())).expect("link");
    denied(A1Lock::acquire(other.path()));
    assert!(
        fs::read_dir(elsewhere.path())
            .expect("list")
            .next()
            .is_none()
    );

    let open = tempfile::tempdir().expect("tempdir");
    fs::create_dir(open.path().join(a1_lock_dir())).expect("dir");
    fs::set_permissions(
        open.path().join(a1_lock_dir()),
        fs::Permissions::from_mode(0o755),
    )
    .expect("chmod");
    denied(A1Lock::acquire(open.path()));
    assert!(!open.path().join(a1_lock_dir()).join(A1_LOCK_FILE).exists());

    let kept = tempfile::tempdir().expect("tempdir");
    let private = kept.path().join(a1_lock_dir());
    fs::create_dir(&private).expect("dir");
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700)).expect("chmod");
    fs::write(private.join(A1_LOCK_FILE), b"owner note").expect("lock file");
    drop(A1Lock::acquire(kept.path()).expect("an existing private lock"));
    assert_eq!(
        fs::read(private.join(A1_LOCK_FILE)).expect("lock file"),
        b"owner note"
    );
}

#[test]
fn a_directory_pre_created_at_a_shared_name_does_not_block_the_lock() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let squat = dir.path().join("jackin-e2e-a1");
    fs::create_dir(&squat).expect("dir");
    fs::set_permissions(&squat, fs::Permissions::from_mode(0o777)).expect("chmod");
    let held = A1Lock::acquire(dir.path()).expect("the per-user directory is free");
    assert!(dir.path().join(a1_lock_dir()).join(A1_LOCK_FILE).exists());
    assert!(!squat.join(A1_LOCK_FILE).exists());
    drop(held);
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

#[tokio::test]
async fn a_needle_in_a_setup_error_is_scrubbed_from_stderr() {
    let scenario = Scenario::new().await;
    scenario.knobs().host_spawn_error =
        Some(("gateway add".to_owned(), format!("boom {ACCESS} {REFRESH}")));
    let ran = scenario.run().await;
    assert_eq!(ran.status.code(), 2, "{}", ran.stderr);
    assert!(
        ran.stderr.contains("jackin gateway add could not run"),
        "the error reaches stderr: {}",
        ran.stderr
    );
    assert!(
        !ran.stderr.contains(ACCESS) && !ran.stderr.contains(REFRESH),
        "stderr carries a token: {}",
        ran.stderr
    );
}

#[tokio::test]
async fn a_step_panic_never_reaches_a_panic_hook_installed_before_the_run() {
    let heard = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = std::sync::Arc::clone(&heard);
    std::panic::set_hook(Box::new(move |info| {
        sink.lock().expect("lock").push_str(&info.to_string());
    }));
    let panics = Scenario::new().await;
    panics.knobs().panic_in_probe = Some(format!("boom {ACCESS}"));
    let ran = panics.run().await;
    drop(std::panic::take_hook());
    assert_eq!(ran.status_of("J04"), "FAIL", "{}", ran.stdout);
    let text = heard.lock().expect("lock").clone();
    assert!(
        !text.contains(ACCESS),
        "the hook installed before the run heard the token: {text}"
    );
}
