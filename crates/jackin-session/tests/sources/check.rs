// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{FileMode, FileSystem};
use jackin_session::{FolderFacts, OpFault, Source, SourceFault, SourceFound};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::support::{
    ACCOUNT_ID, Bench, CANARY, TestResult, chatgpt_login, concealed, exited, failed, guard, host,
    item,
};

fn token() -> CancellationToken {
    CancellationToken::new()
}

#[tokio::test]
async fn an_env_source_needs_a_valid_name_and_a_set_non_empty_variable() -> TestResult {
    guard(async {
        let bench = Bench::new().env("SET_KEY", CANARY).env("EMPTY_KEY", "  ");
        let desk = bench.desk();
        let cancel = token();
        assert_eq!(
            desk.check(Source::Env { var: "SET_KEY" }, &cancel).await?,
            SourceFound::Env
        );
        let unset = desk.check(Source::Env { var: "NOPE_KEY" }, &cancel).await;
        assert!(matches!(&unset, Err(SourceFault::EnvUnset(var)) if var == "NOPE_KEY"));
        let empty = desk.check(Source::Env { var: "EMPTY_KEY" }, &cancel).await;
        assert!(matches!(&empty, Err(SourceFault::EnvEmpty(_))));
        let bad = desk.check(Source::Env { var: "not a name" }, &cancel).await;
        assert!(matches!(&bad, Err(SourceFault::EnvName(_))));
        let shown = format!("{unset:?}{empty:?}{bad:?}");
        assert!(
            !shown.contains(CANARY),
            "the value reached an error: {shown}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_file_source_must_be_a_regular_file() -> TestResult {
    guard(async {
        let bench = Bench::new();
        let dir = host("/creds")?;
        bench.fs.create_dir_all(&dir).await?;
        let file = host("/creds/key.txt")?;
        bench.fs.write_new(&file, b"token", FileMode(0o600)).await?;
        let desk = bench.desk();
        let cancel = token();
        assert_eq!(
            desk.check(Source::File { path: &file }, &cancel).await?,
            SourceFound::File
        );
        let missing = host("/creds/none.txt")?;
        assert!(matches!(
            desk.check(Source::File { path: &missing }, &cancel).await,
            Err(SourceFault::FileMissing(_))
        ));
        assert!(matches!(
            desk.check(Source::File { path: &dir }, &cancel).await,
            Err(SourceFault::FileNotRegular(_))
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_folder_source_reads_the_chatgpt_login_only_when_a_login_is_needed() -> TestResult {
    guard(async {
        let bench = Bench::new();
        let good = host("/home/good")?;
        bench.login(&good, &chatgpt_login().to_string()).await?;
        let broken = host("/home/broken")?;
        bench.login(&broken, "{ not json").await?;
        let apikey = host("/home/apikey")?;
        bench
            .login(&apikey, &json!({"auth_mode": "apikey"}).to_string())
            .await?;
        let empty = host("/home/empty")?;
        bench.fs.create_dir_all(&empty).await?;
        let desk = bench.desk();
        let cancel = token();
        let found = desk
            .check(
                Source::Folder {
                    path: &good,
                    login: true,
                },
                &cancel,
            )
            .await?;
        assert!(matches!(&found, SourceFound::Login { account } if account.as_str() == ACCOUNT_ID));
        let wanted = |path, login| desk.check(Source::Folder { path, login }, &cancel);
        assert!(matches!(
            wanted(&broken, true).await,
            Err(SourceFault::LoginInvalid(_))
        ));
        assert!(matches!(
            wanted(&apikey, true).await,
            Err(SourceFault::NotChatgpt(_))
        ));
        assert!(matches!(
            wanted(&empty, true).await,
            Err(SourceFault::NoLogin(_))
        ));
        let gone = host("/home/gone")?;
        assert!(matches!(
            wanted(&gone, true).await,
            Err(SourceFault::FolderMissing(_))
        ));
        assert_eq!(wanted(&empty, false).await?, SourceFound::Folder);
        assert_eq!(wanted(&broken, false).await?, SourceFound::Folder);
        let shown = format!("{found:?}");
        assert!(!shown.contains("ACCESS-FAKE-1"), "{shown}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn probing_a_folder_names_what_it_holds() -> TestResult {
    guard(async {
        let bench = Bench::new();
        let login = host("/p/login")?;
        bench.login(&login, &chatgpt_login().to_string()).await?;
        let broken = host("/p/broken")?;
        bench.login(&broken, "[").await?;
        let other = host("/p/other")?;
        bench
            .login(&other, &json!({"auth_mode": "apikey"}).to_string())
            .await?;
        let empty = host("/p/empty")?;
        bench.fs.create_dir_all(&empty).await?;
        let file = host("/p/file")?;
        bench.fs.write_new(&file, b"x", FileMode(0o600)).await?;
        let desk = bench.desk();
        let facts = [
            desk.probe_folder(&login).await,
            desk.probe_folder(&broken).await,
            desk.probe_folder(&other).await,
            desk.probe_folder(&empty).await,
            desk.probe_folder(&file).await,
            desk.probe_folder(&host("/p/gone")?).await,
        ];
        assert_eq!(
            facts,
            [
                FolderFacts::CodexLogin,
                FolderFacts::Malformed,
                FolderFacts::Unrecognised,
                FolderFacts::NoCredential,
                FolderFacts::Unreadable,
                FolderFacts::Missing
            ]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_reference_that_is_not_a_field_path_never_reaches_op() -> TestResult {
    guard(async {
        let bench = Bench::new();
        let desk = bench.desk();
        let cancel = token();
        for reference in [
            "vault/item/field",
            "op://vault",
            "op://vault/item",
            "op://a/b/c/d/e",
            "op://vault//field",
        ] {
            let result = desk
                .check(
                    Source::OnePassword {
                        account: "acct-1",
                        reference,
                    },
                    &cancel,
                )
                .await;
            assert!(
                matches!(&result, Err(SourceFault::Reference(text)) if text == reference),
                "{reference}: {result:?}"
            );
        }
        assert!(
            bench.op_calls().is_empty(),
            "op ran for a malformed reference"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_reference_with_a_segment_that_starts_with_a_dash_never_reaches_op() -> TestResult {
    guard(async {
        let bench = Bench::new();
        let desk = bench.desk();
        let cancel = token();
        for reference in [
            "op://v/--reveal/f1",
            "op://-v/item/f1",
            "op://v/item/-s/f1",
            "op://v/item/s/--f",
        ] {
            let result = desk
                .check(
                    Source::OnePassword {
                        account: "acct-1",
                        reference,
                    },
                    &cancel,
                )
                .await;
            assert!(
                matches!(&result, Err(SourceFault::Reference(text)) if text == reference),
                "{reference}: {result:?}"
            );
        }
        assert!(bench.op_calls().is_empty(), "{:?}", bench.op_calls());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_reference_is_checked_against_the_item_op_reads() -> TestResult {
    guard(async {
        let fields = json!([
            concealed("key", "api key", CANARY),
            concealed("blank", "blank key", ""),
            {"id": "url", "label": "endpoint", "type": "URL", "value": "https://x.test"},
            {"id": "sec", "label": "sectioned", "type": "CONCEALED", "value": CANARY, "section": {"id": "s1"}},
        ]);
        let bench = Bench::new();
        bench.op(&["item", "get"], exited(&item(&fields)));
        let desk = bench.desk();
        let cancel = token();
        let check = |reference: &'static str| {
            desk.check(
                Source::OnePassword {
                    account: "acct-1",
                    reference,
                },
                &cancel,
            )
        };
        assert_eq!(check("op://vault-1/item-1/key").await?, SourceFound::OnePassword);
        assert_eq!(check("op://vault-1/item-1/s1/sec").await?, SourceFound::OnePassword);
        assert!(matches!(
            check("op://vault-1/item-1/url").await,
            Err(SourceFault::NotConcealed(label)) if label == "endpoint"
        ));
        assert!(matches!(
            check("op://vault-1/item-1/blank").await,
            Err(SourceFault::EmptyField(label)) if label == "blank key"
        ));
        assert!(matches!(
            check("op://vault-1/item-1/missing").await,
            Err(SourceFault::Op(OpFault::MissingField { field, .. })) if field == "missing"
        ));
        assert!(matches!(
            check("op://vault-1/item-1/sec").await,
            Err(SourceFault::Op(OpFault::MissingField { .. }))
        ));
        let calls = bench.op_calls();
        assert!(calls.iter().all(|c| c.args[..2] == ["item", "get"]));
        assert!(calls.iter().all(|c| c.args.contains(&"acct-1".to_owned())));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_locked_op_fails_the_check_with_the_locked_fault_and_no_text() -> TestResult {
    guard(async {
        let bench = Bench::new();
        bench.op(
            &["item", "get"],
            failed(&format!("You are not currently signed in {CANARY}")),
        );
        let result = bench
            .desk()
            .check(
                Source::OnePassword {
                    account: "acct-1",
                    reference: "op://vault-1/item-1/key",
                },
                &token(),
            )
            .await;
        assert!(
            matches!(&result, Err(SourceFault::Op(OpFault::Locked))),
            "{result:?}"
        );
        let shown = format!(
            "{result:?}{}",
            result
                .as_ref()
                .err()
                .map(ToString::to_string)
                .unwrap_or_default()
        );
        assert!(!shown.contains(CANARY), "{shown}");
        Ok(())
    })
    .await
}
