// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;
use std::os::unix::fs::symlink;
use std::process::Command;

use jackin_core::{CREDENTIAL_FILE_MAX_BYTES, FsError, HostPath, StdFileSystem};
use jackin_session::{CredentialError, read_chatgpt_login};
use jackin_testkit::{FsOp, MemFs};
use serde_json::{Value, json};

use super::support::{
    ACCESS, ACCOUNT_ID, CLIENT, REFRESH, SECRETS, TestResult, guard, host, login_json, put,
};

fn assert_clean(text: &str) {
    for secret in SECRETS {
        assert!(!text.contains(secret), "{secret} leaked into {text}");
    }
}

#[tokio::test]
async fn a_real_shaped_codex_login_parses_into_secrets_and_the_account_id_without_leaking()
-> TestResult {
    guard(async {
        let mut extended = login_json();
        extended["extra"] = json!({"nested": true});
        extended["tokens"]["extra"] = json!(1);
        extended["tokens"]["oauth_provenance"]["extra"] = json!("x");
        for (index, text) in [login_json().to_string(), extended.to_string()]
            .iter()
            .enumerate()
        {
            let fs = MemFs::new();
            let home = host(&format!("/home/row{index}/.codex"))?;
            put(&fs, &home, text).await?;
            let login = read_chatgpt_login(&fs, &home).await?;
            assert_eq!(login.access_token.expose(), ACCESS);
            assert_eq!(login.refresh_token.expose(), REFRESH);
            assert_eq!(login.client_id, CLIENT);
            assert_eq!(login.account_id.as_str(), ACCOUNT_ID);
            assert_clean(&format!("{login:?}"));
        }
        Ok(())
    })
    .await
}

fn without(path: &[&str]) -> Value {
    let mut login = login_json();
    let mut node = &mut login;
    for key in &path[..path.len() - 1] {
        node = &mut node[*key];
    }
    if let Some(object) = node.as_object_mut() {
        object.remove(path[path.len() - 1]);
    }
    login
}

async fn read_error(text: &str) -> Result<CredentialError, Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let home = host("/home/me/.codex")?;
    put(&fs, &home, text).await?;
    match read_chatgpt_login(&fs, &home).await {
        Ok(login) => Err(format!("the login parsed: {login:?}").into()),
        Err(error) => Ok(error),
    }
}

#[tokio::test]
async fn a_missing_unreadable_or_malformed_login_is_a_typed_secret_free_error() -> TestResult {
    guard(async {
        let home = host("/home/me/.codex")?;
        let missing = MemFs::new();
        let error = read_chatgpt_login(&missing, &home).await.err();
        assert!(
            matches!(&error, Some(CredentialError::LoginUnreadable { path }) if path.as_path().ends_with(".codex/auth.json")),
            "{error:?}"
        );

        let unreadable = MemFs::new();
        put(&unreadable, &home, &login_json().to_string()).await?;
        unreadable.fail_at(
            FsOp::Read,
            1,
            FsError::Io {
                path: home.clone(),
                kind: ErrorKind::PermissionDenied,
                message: "denied".to_owned(),
            },
        );
        let error = read_chatgpt_login(&unreadable, &home).await.err();
        assert!(
            matches!(&error, Some(CredentialError::Fs(FsError::Io { kind: ErrorKind::PermissionDenied, .. }))),
            "{error:?}"
        );

        let mut errors = vec![];
        let truncated = login_json().to_string();
        errors.push((read_error(&truncated[..truncated.len() - 20]).await?, None));
        for (path, key) in [
            (&["tokens", "access_token"][..], "tokens.access_token"),
            (&["tokens", "refresh_token"][..], "tokens.refresh_token"),
            (&["tokens", "oauth_provenance", "client_id"][..], "tokens.oauth_provenance.client_id"),
            (&["tokens", "account_id"][..], "tokens.account_id"),
        ] {
            errors.push((read_error(&without(path).to_string()).await?, Some(key)));
        }
        let mut string_tokens = login_json();
        string_tokens["tokens"] = json!(ACCESS);
        errors.push((read_error(&string_tokens.to_string()).await?, Some("tokens")));
        let mut object_refresh = login_json();
        object_refresh["tokens"]["refresh_token"] = json!({ "value": REFRESH });
        errors.push((read_error(&object_refresh.to_string()).await?, Some("tokens.refresh_token")));
        for bad in ["a_b", "openshell:resolve:env:CODEX_AUTH_ACCOUNT_ID"] {
            let mut login = login_json();
            login["tokens"]["account_id"] = json!(bad);
            errors.push((read_error(&login.to_string()).await?, Some("tokens.account_id")));
        }
        for (error, key) in &errors {
            match (error, key) {
                (CredentialError::AuthJsonInvalid { key: found }, Some(wanted)) => {
                    assert_eq!(found, wanted);
                }
                (CredentialError::AuthJsonInvalid { .. }, None) => {}
                other => return Err(format!("unexpected {other:?}").into()),
            }
            assert_clean(&format!("{error} {error:?}"));
        }

        let api = r#"{"auth_mode":"apikey","OPENAI_API_KEY":"API-SECRET-91b2","tokens":null}"#;
        let error = read_error(api).await?;
        assert!(matches!(error, CredentialError::NotAChatgptLogin), "{error:?}");
        assert_clean(&format!("{error} {error:?}"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_linked_login_is_read_and_a_pipe_or_oversized_login_is_refused_without_reading_it()
-> TestResult {
    guard(async {
        let dir = tempfile::tempdir()?;
        let base = std::fs::canonicalize(dir.path())?;
        let fs = StdFileSystem::default();
        let real = base.join("real.json");
        std::fs::write(&real, login_json().to_string())?;
        let linked = base.join("linked");
        std::fs::create_dir(&linked)?;
        symlink(&real, linked.join("auth.json"))?;
        let login = read_chatgpt_login(&fs, &HostPath::parse(&linked)?).await?;
        assert_eq!(login.account_id.as_str(), ACCOUNT_ID);

        let piped = base.join("piped");
        std::fs::create_dir(&piped)?;
        assert!(
            Command::new("mkfifo")
                .arg(piped.join("auth.json"))
                .status()?
                .success()
        );
        let error = read_chatgpt_login(&fs, &HostPath::parse(&piped)?)
            .await
            .err();
        assert!(
            matches!(error, Some(CredentialError::LoginNotRegular { .. })),
            "{error:?}"
        );

        let big = base.join("big");
        std::fs::create_dir(&big)?;
        std::fs::write(
            big.join("auth.json"),
            vec![b' '; CREDENTIAL_FILE_MAX_BYTES + 1],
        )?;
        let error = read_chatgpt_login(&fs, &HostPath::parse(&big)?).await.err();
        assert!(
            matches!(error, Some(CredentialError::LoginTooLarge { .. })),
            "{error:?}"
        );
        Ok(())
    })
    .await
}
