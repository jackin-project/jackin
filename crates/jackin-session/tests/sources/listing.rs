// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::{OpFault, OpFieldKind};
use jackin_testkit::ProcessReply;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::support::{Bench, CANARY, TestResult, concealed, exited, failed, guard, item};

fn token() -> CancellationToken {
    CancellationToken::new()
}

#[tokio::test]
async fn the_four_listings_parse_and_the_value_never_survives_the_item_read() -> TestResult {
    guard(async {
        let bench = Bench::new();
        bench.op(
            &["account", "list"],
            exited(&json!([{"account_uuid": "acct-1", "email": "me@example.test", "url": "x"}])),
        );
        bench.op(
            &["vault", "list"],
            exited(&json!([{"id": "vault-1", "name": "Private"}])),
        );
        bench.op(
            &["item", "list"],
            exited(&json!([{"id": "item-1", "title": "OpenAI", "category": "API_CREDENTIAL"}])),
        );
        bench.op(
            &["item", "get"],
            exited(&item(&json!([
                concealed("f1", "api key", CANARY),
                {"id": "f2", "label": "endpoint", "type": "URL", "value": "https://x.test"},
                {"id": "f3", "type": "STRING", "value": "plain"},
                {"id": "f4", "label": "in section", "type": "CONCEALED", "value": "", "section": {"id": "sec-1"}},
            ]))),
        );
        let cli = bench.cli();
        let accounts = cli.accounts(&token()).await?;
        let vaults = cli.vaults("acct-1", &token()).await?;
        let items = cli.items("acct-1", "vault-1", &token()).await?;
        let detail = cli.item("acct-1", "vault-1", "item-1", &token()).await?;

        assert_eq!(accounts[0].id, "acct-1");
        assert_eq!(accounts[0].email, "me@example.test");
        assert_eq!(vaults[0].name, "Private");
        assert_eq!(items[0].category, "API_CREDENTIAL");
        let kinds: Vec<_> = detail.fields.iter().map(|f| f.kind).collect();
        assert_eq!(
            kinds,
            [
                OpFieldKind::Concealed,
                OpFieldKind::Url,
                OpFieldKind::Text,
                OpFieldKind::Concealed
            ]
        );
        assert_eq!(detail.fields[0].tail.as_str(), "d2e8");
        assert_eq!(detail.fields[2].label, "f3");
        assert_eq!(detail.fields[3].section.as_deref(), Some("sec-1"));
        assert!(detail.fields[3].empty);
        assert!(!detail.fields[0].empty);
        let shown = format!("{accounts:?}{vaults:?}{items:?}{detail:?}");
        assert!(!shown.contains(CANARY), "{shown}");
        assert!(!shown.contains("d2e8"), "{shown}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn op_gets_the_json_format_and_the_chosen_account_only_when_one_is_named() -> TestResult {
    guard(async {
        let bench = Bench::new();
        bench.op(&["vault", "list"], exited(&json!([])));
        let cli = bench.cli();
        cli.vaults("acct-1", &token()).await?;
        cli.vaults("", &token()).await?;
        let calls = bench.op_calls();
        assert_eq!(
            calls[0].args,
            ["vault", "list", "--account", "acct-1", "--format", "json"]
        );
        assert_eq!(calls[1].args, ["vault", "list", "--format", "json"]);
        Ok(())
    })
    .await
}

type Expect = fn(&OpFault) -> bool;

fn classified(stderr: &str) -> String {
    format!("{stderr} {CANARY}")
}

#[tokio::test]
async fn a_failed_op_is_classified_and_its_stderr_is_never_kept() -> TestResult {
    guard(async {
        let cases: [(&str, Expect); 7] = [
            ("You are not currently signed in", |f| {
                matches!(f, OpFault::Locked)
            }),
            ("authorization prompt dismissed", |f| {
                matches!(f, OpFault::AuthorizationRequired { account } if account == "acct-1")
            }),
            ("You do not have permission to read", |f| {
                matches!(f, OpFault::PermissionDenied { vault } if vault == "vault-1")
            }),
            ("\"vault-1\" isn't a vault in this account", |f| {
                matches!(f, OpFault::MissingVault { vault } if vault == "vault-1")
            }),
            ("\"item-1\" isn't an item in the \"vault-1\" vault", |f| {
                matches!(f, OpFault::MissingItem { item, vault } if item == "item-1" && vault == "vault-1")
            }),
            ("\"acct-1\" isn't an account on this device", |f| {
                matches!(f, OpFault::MissingAccount { account } if account == "acct-1")
            }),
            ("something else went wrong", |f| matches!(f, OpFault::Failed)),
        ];
        for (marker, expected) in cases {
            let bench = Bench::new();
            bench.op(&["item", "get"], failed(&classified(marker)));
            let fault = bench
                .cli()
                .item("acct-1", "vault-1", "item-1", &token())
                .await
                .err()
                .ok_or("op failed but the call succeeded")?;
            assert!(expected(&fault), "{marker}: {fault:?}");
            let shown = format!("{fault:?} {fault}");
            assert!(!shown.contains(CANARY), "{marker}: {shown}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_missing_op_binary_and_unreadable_output_are_told_apart() -> TestResult {
    guard(async {
        let none = Bench::new();
        let fault = none.cli().accounts(&token()).await.err().ok_or("ok")?;
        assert!(matches!(fault, OpFault::NotInstalled(_)), "{fault:?}");

        let bad = Bench::new();
        bad.op(
            &["account", "list"],
            ProcessReply::exited(0, format!("{CANARY} not json").as_bytes(), b""),
        );
        let fault = bad.cli().accounts(&token()).await.err().ok_or("ok")?;
        assert!(matches!(fault, OpFault::Malformed), "{fault:?}");
        assert!(!format!("{fault:?}{fault}").contains(CANARY));

        let wrong = Bench::new();
        wrong.op(&["account", "list"], exited(&json!({"not": "a list"})));
        let fault = wrong.cli().accounts(&token()).await.err().ok_or("ok")?;
        assert!(matches!(fault, OpFault::Malformed), "{fault:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_concealed_value_of_eight_characters_or_fewer_shows_no_tail() -> TestResult {
    guard(async {
        let bench = Bench::new();
        bench.op(
            &["item", "get"],
            exited(&item(&json!([
                concealed("f1", "three", "abc"),
                concealed("f2", "eight", "12345678"),
                concealed("f3", "nine", "123456789"),
                concealed("f4", "none", ""),
            ]))),
        );
        let detail = bench
            .cli()
            .item("acct-1", "vault-1", "item-1", &token())
            .await?;
        let tails: Vec<&str> = detail.fields.iter().map(|f| f.tail.as_str()).collect();
        assert_eq!(tails, ["", "", "6789", ""]);
        let shown = format!("{detail:?}");
        assert!(
            !shown.contains("abc") && !shown.contains("12345678") && !shown.contains("6789"),
            "{shown}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_name_that_starts_with_a_dash_never_reaches_op_and_the_item_follows_a_double_dash()
-> TestResult {
    guard(async {
        let bench = Bench::new();
        bench.op(&["item", "get"], exited(&item(&json!([]))));
        bench.op(&["item", "list"], exited(&json!([])));
        bench.op(&["vault", "list"], exited(&json!([])));
        let cli = bench.cli();
        for refused in [
            cli.item("acct-1", "vault-1", "--reveal", &token())
                .await
                .err(),
            cli.item("acct-1", "-vault", "item-1", &token()).await.err(),
            cli.item("-acct", "vault-1", "item-1", &token()).await.err(),
            cli.items("acct-1", "--cache", &token()).await.err(),
            cli.vaults("--session", &token()).await.err(),
        ] {
            assert!(
                matches!(refused, Some(OpFault::FlagLikeName { .. })),
                "{refused:?}"
            );
        }
        assert!(bench.op_calls().is_empty(), "{:?}", bench.op_calls());
        cli.item("acct-1", "vault-1", "item-1", &token()).await?;
        assert_eq!(
            bench.op_calls()[0].args,
            [
                "item",
                "get",
                "--vault",
                "vault-1",
                "--account",
                "acct-1",
                "--format",
                "json",
                "--",
                "item-1"
            ]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_name_that_holds_a_marker_word_does_not_change_the_fault() -> TestResult {
    guard(async {
        let cases: [(&str, &str, Expect); 3] = [
            (
                "approve-keys",
                "\"approve-keys\" isn't an item in the \"vault-1\" vault",
                |f| matches!(f, OpFault::MissingItem { item, .. } if item == "approve-keys"),
            ),
            (
                "forbidden",
                "\"forbidden\" isn't an item in the \"vault-1\" vault",
                |f| matches!(f, OpFault::MissingItem { item, .. } if item == "forbidden"),
            ),
            (
                "not signed in",
                "\"not signed in\" isn't an item in the \"vault-1\" vault",
                |f| matches!(f, OpFault::MissingItem { item, .. } if item == "not signed in"),
            ),
        ];
        for (name, stderr, expected) in cases {
            let bench = Bench::new();
            bench.op(&["item", "get"], failed(stderr));
            let fault = bench
                .cli()
                .item("acct-1", "vault-1", name, &token())
                .await
                .err()
                .ok_or("op failed but the call succeeded")?;
            assert!(expected(&fault), "{name}: {fault:?}");
        }
        Ok(())
    })
    .await
}
