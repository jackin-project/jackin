// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::{OpError, OpSession};
use jackin_core::{CredentialSource, Program};
use jackin_testkit::{ProcessReply, ScriptedProgram, TestResult, guard};
use serde_json::{Value, json};

use super::support::{CANARY, Lab};

fn reply(value: &Value) -> ProcessReply {
    ProcessReply::exited(0, value.to_string().as_bytes(), b"")
}

fn refusal(text: &str) -> ProcessReply {
    ProcessReply::exited(1, b"", format!("{text} {CANARY}").as_bytes())
}

fn item() -> Value {
    json!({
        "id": "item-1",
        "title": "OpenAI",
        "category": "API_CREDENTIAL",
        "fields": [
            {"id": "key", "label": "api key", "type": "CONCEALED", "value": CANARY},
            {"id": "url", "label": "endpoint", "type": "URL", "value": "https://api.example.test"},
        ],
    })
}

fn script(lab: &Lab) {
    let op = lab.rig.world.runner.scripted();
    let on = |args: &[&str], reply| op.expect(ScriptedProgram::Op, args, reply);
    on(
        &["account", "list"],
        reply(&json!([{"account_uuid": "acct-1", "email": "me@example.test"}])),
    );
    on(
        &["vault", "list"],
        reply(&json!([
            {"id": "vault-1", "name": "Private"},
            {"id": "vault-2", "name": "Shared"},
        ])),
    );
    on(
        &["item", "list", "--vault", "vault-1"],
        reply(&json!([{"id": "item-1", "title": "OpenAI", "category": "API_CREDENTIAL"}])),
    );
    on(
        &["item", "list", "--vault", "vault-2"],
        refusal("You do not have permission to read this vault"),
    );
    on(&["item", "get"], reply(&item()));
}

async fn picker(lab: &mut Lab) -> TestResult {
    lab.open_form("spare").await?;
    lab.choose_one_password().await?;
    lab.step("tab").await?;
    lab.console.keys("enter")?;
    Ok(())
}

#[tokio::test]
async fn the_picker_walks_to_a_concealed_field_and_the_form_saves_only_its_reference() -> TestResult
{
    guard(async {
        let mut lab = Lab::open().await?;
        script(&lab);
        let mut frames = Vec::new();
        picker(&mut lab).await?;
        let loading = lab.console.screen();
        assert!(loading.contains("loading accounts"), "{loading}");
        frames.push(loading);
        lab.console.settle().await?;
        let accounts = lab.console.screen();
        assert!(accounts.contains("acct-1") && accounts.contains("me@example.test"), "{accounts}");
        frames.push(accounts);
        lab.step("enter").await?;
        let vaults = lab.console.screen();
        assert!(vaults.contains("Private") && vaults.contains("1 items"), "{vaults}");
        assert!(vaults.contains("Shared") && vaults.contains("no access"), "{vaults}");
        frames.push(vaults);
        lab.step("enter").await?;
        let items = lab.console.screen();
        assert!(items.contains("OpenAI") && items.contains("api credential"), "{items}");
        frames.push(items);
        lab.step("enter").await?;
        let fields = lab.console.screen();
        assert!(fields.contains("api key") && fields.contains("concealed"), "{fields}");
        assert!(fields.contains("endpoint") && fields.contains("url"), "{fields}");
        frames.push(fields);
        lab.step("enter").await?;
        let chosen = lab.console.screen();
        assert!(chosen.contains("Private › OpenAI › api key"), "{chosen}");
        assert!(chosen.contains("••••••••…d2e8"), "{chosen}");
        frames.push(chosen);
        lab.step("tab tab tab tab enter").await?;
        frames.push(lab.console.screen());

        let config = lab.config().await?;
        let saved = config.accounts().find(|a| a.name().as_str() == "spare").ok_or("saved")?;
        assert!(matches!(
            saved.credential(),
            CredentialSource::OnePassword { reference } if reference.as_str() == "op://vault-1/item-1/key"
        ));

        let config_text = lab.console.config_text().await?;
        assert!(!config_text.contains(CANARY), "{config_text}");
        for frame in &frames {
            assert!(!frame.contains(CANARY), "{frame}");
        }
        let op = format!("{:?}", lab.world().op);
        assert!(!op.contains(CANARY), "{op}");
        let accounts = format!("{:?}", lab.world().accounts);
        assert!(!accounts.contains(CANARY), "{accounts}");
        for call in lab
            .rig
            .world
            .runner
            .scripted()
            .seen()
            .into_iter()
            .filter(|call| call.program == Program::Op)
        {
            assert!(!format!("{call:?}").contains(CANARY), "{call:?}");
            assert!(call.args.first().is_some_and(|a| a != "read"), "{call:?}");
        }
        assert!(lab.rig.world.gateway.providers().len() <= 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_locked_op_shows_the_baseline_text_and_a_retry_after_unlocking_lists_accounts()
-> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let op = lab.rig.world.runner.scripted();
        op.expect(
            ScriptedProgram::Op,
            &["account", "list"],
            refusal("You are not currently signed in"),
        );
        op.expect(
            ScriptedProgram::Op,
            &["account", "list"],
            reply(&json!([{"account_uuid": "acct-1", "email": "me@example.test"}])),
        );
        picker(&mut lab).await?;
        lab.console.settle().await?;
        let locked = lab.console.screen();
        assert!(locked.contains("1Password is locked"), "{locked}");
        assert!(locked.contains("press r to retry"), "{locked}");
        assert!(!locked.contains(CANARY), "{locked}");
        assert_eq!(
            jackin_console::backend::OnePassword::session(lab.world().op.as_ref()),
            OpSession::Locked
        );
        lab.step("r").await?;
        let retried = lab.console.screen();
        assert!(retried.contains("acct-1"), "{retried}");
        assert!(!retried.contains("is locked"), "{retried}");
        assert_eq!(
            jackin_console::backend::OnePassword::session(lab.world().op.as_ref()),
            OpSession::SignedIn
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn authorization_and_a_denied_vault_each_show_their_text() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let op = lab.rig.world.runner.scripted();
        op.expect(
            ScriptedProgram::Op,
            &["account", "list"],
            refusal("authorization prompt dismissed"),
        );
        picker(&mut lab).await?;
        lab.console.settle().await?;
        let screen = lab.console.screen();
        assert!(
            screen.contains("not authorized for 1Password CLI"),
            "{screen}"
        );
        assert!(!screen.contains(CANARY), "{screen}");

        let mut lab = Lab::open().await?;
        script(&lab);
        picker(&mut lab).await?;
        lab.console.settle().await?;
        lab.step("enter").await?;
        lab.step("down").await?;
        lab.step("enter").await?;
        let denied = lab.console.screen();
        assert!(denied.contains("No access to vault vault-2"), "{denied}");
        assert!(!denied.contains(CANARY), "{denied}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_field_that_is_not_concealed_is_refused_with_the_baseline_wording() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        script(&lab);
        picker(&mut lab).await?;
        lab.console.settle().await?;
        for _ in 0..3 {
            lab.step("enter").await?;
        }
        let op = lab.world().op.box_clone();
        let kept = op
            .reference("acct-1", "vault-1", "item-1", "api key")
            .map_err(|e| e.message())?;
        assert_eq!(kept.canonical(), "op://vault-1/item-1/key");
        assert_eq!(kept.vault_name, "Private");
        assert_eq!(kept.item_title, "OpenAI");
        assert_eq!(
            op.describe(&kept).map_err(|e| e.message())?.masked,
            "••••••••…d2e8"
        );
        let refused = op.reference("acct-1", "vault-1", "item-1", "endpoint");
        assert_eq!(
            refused,
            Err(OpError::WrongFieldShape {
                field: "endpoint".to_owned()
            })
        );
        assert_eq!(
            refused.err().map(|e| e.message()),
            Some("Field endpoint is not a credential: choose a concealed field".to_owned())
        );
        Ok(())
    })
    .await
}
