// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{CredentialSource, Provider};
use jackin_testkit::{ACCOUNT_ID, ProcessReply, ScriptedProgram, TestResult, guard};

use super::support::{CANARY, Lab};

async fn login_folder(lab: &Lab) -> Result<String, Box<dyn std::error::Error>> {
    let folder = lab.code.join("login");
    tokio::fs::create_dir_all(&folder).await?;
    tokio::fs::copy(
        lab.rig.home.as_path().join("auth.json"),
        folder.join("auth.json"),
    )
    .await?;
    Ok(folder.display().to_string())
}

async fn codex_folder_form(lab: &mut Lab, folder: &str) -> TestResult {
    lab.open_form("spare").await?;
    lab.choose_codex().await?;
    lab.step("tab").await?;
    lab.step("tab").await?;
    lab.console.type_text(folder);
    lab.console.settle().await?;
    Ok(())
}

#[tokio::test]
async fn the_live_form_labels_the_sources_and_preselects_the_local_folder() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.step("c").await?;
        lab.step("a").await?;
        let form = lab.console.screen();
        assert!(
            form.contains("( ) 1Password item / field  (usage only)"),
            "{form}"
        );
        assert!(
            form.contains("(●) Local agent folder  (recommended)"),
            "{form}"
        );
        assert!(form.contains("( ) Plain-text API key"), "{form}");
        assert!(
            !form.contains("recommended)\n")
                || !form.contains("1Password item / field  (recommended)")
        );
        assert!(!form.contains("Value for the key"), "{form}");
        lab.step("enter").await?;
        lab.step("tab").await?;
        lab.step("tab").await?;
        lab.step("tab").await?;
        lab.step("up").await?;
        lab.step("space").await?;
        let one_password = lab.console.screen();
        assert!(
            one_password.contains("(●) 1Password item / field"),
            "{one_password}"
        );
        assert!(
            one_password.contains("Usage only · launching Codex needs a local login"),
            "{one_password}"
        );
        assert!(
            !one_password.contains("API key  optional"),
            "{one_password}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn validate_waits_and_then_reports_a_codex_folder_row_by_row() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let home = login_folder(&lab).await?;
        codex_folder_form(&mut lab, &home).await?;
        lab.console.keys("tab tab enter")?;
        let pending = lab.console.screen();
        assert!(pending.contains("checking"), "{pending}");
        assert!(!pending.contains("✓ material"), "{pending}");
        lab.console.settle().await?;
        let done = lab.console.screen();
        assert!(done.contains("✓ material   Codex auth.json"), "{done}");
        assert!(!done.contains("checking"), "{done}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_folder_without_a_login_fails_the_material_row_and_blocks_the_save() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let empty = lab.code.join("alpha").display().to_string();
        let before = lab.console.config_text().await?;
        codex_folder_form(&mut lab, &empty).await?;
        lab.step("tab tab enter").await?;
        let failed = lab.console.screen();
        assert!(failed.contains("! material"), "{failed}");
        assert!(!failed.contains("✓ material"), "{failed}");
        assert!(
            failed.contains("Validation failed · fix the source and try again"),
            "{failed}"
        );
        lab.step("tab tab enter").await?;
        assert_eq!(lab.console.config_text().await?, before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn saving_a_codex_folder_through_the_form_checks_it_and_stores_the_login() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let home = login_folder(&lab).await?;
        codex_folder_form(&mut lab, &home).await?;
        lab.step("tab tab tab tab enter").await?;
        let config = lab.config().await?;
        let saved = config
            .accounts()
            .find(|account| account.name().as_str() == "spare")
            .ok_or("the account was not saved")?;
        assert_eq!(saved.provider(), Provider::Codex);
        assert!(matches!(
            saved.credential(),
            CredentialSource::CodexLogin { chatgpt_account_id, .. } if chatgpt_account_id.as_str() == ACCOUNT_ID
        ));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_locked_op_fails_the_check_with_the_baseline_text_and_a_retry_passes() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let op = lab.rig.world.runner.scripted();
        let ok = |text: &str| ProcessReply::exited(0, text.as_bytes(), b"");
        let on = |args: &[&str], reply| op.expect(ScriptedProgram::Op, args, reply);
        on(&["account", "list"], ok(r#"[{"account_uuid":"acct-1","email":"me@example.test"}]"#));
        on(&["vault", "list"], ok(r#"[{"id":"vault-1","name":"Private"}]"#));
        on(&["item", "list"], ok(r#"[{"id":"item-1","title":"OpenAI","category":"LOGIN"}]"#));
        let item = format!(
            r#"{{"id":"item-1","title":"OpenAI","fields":[{{"id":"key","label":"api key","type":"CONCEALED","value":"{CANARY}"}}]}}"#
        );
        on(&["item", "get"], ok(&item));
        on(
            &["item", "get"],
            ProcessReply::exited(
                1,
                b"",
                format!("You are not currently signed in {CANARY}").as_bytes(),
            ),
        );
        on(&["item", "get"], ok(&item));
        lab.open_form("spare").await?;
        lab.choose_one_password().await?;
        lab.step("tab").await?;
        lab.step("enter").await?;
        for _ in 0..4 {
            lab.step("enter").await?;
        }
        lab.step("tab tab").await?;
        lab.console.keys("enter")?;
        let pending = lab.console.screen();
        assert!(pending.contains("checking"), "{pending}");
        lab.console.settle().await?;
        let locked = lab.console.screen();
        assert!(locked.contains("! material   1Password locked: unlock the app and retry"), "{locked}");
        assert!(!locked.contains(CANARY), "{locked}");
        lab.step("enter").await?;
        let passed = lab.console.screen();
        assert!(passed.contains("✓ material   Private › OpenAI › api key"), "{passed}");
        assert!(!passed.contains("1Password locked"), "{passed}");
        assert!(!passed.contains(CANARY), "{passed}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_env_account_shows_as_a_host_variable_and_its_edit_is_refused_with_the_reason()
-> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let env = jackin_core::Account::new(
            jackin_core::AccountName::parse("hostkey")?,
            Provider::Claude,
            CredentialSource::Env {
                var: jackin_core::EnvName::parse("CLAUDE_HOST_KEY")?,
            },
        )?;
        lab.edit(vec![jackin_core::ConfigEdit::AddAccount(env)])
            .await?;
        lab.reopen().await?;
        lab.step("c").await?;
        for _ in 0..8 {
            if lab.console.screen().contains("Credential    Host env") {
                break;
            }
            lab.step("down").await?;
        }
        let detail = lab.console.screen();
        assert!(detail.contains("Credential    Host env"), "{detail}");
        assert!(detail.contains("$CLAUDE_HOST_KEY"), "{detail}");
        lab.step("e").await?;
        let refused = lab.console.screen();
        let reason = jackin_console::domain::openshell::env_edit_block("CLAUDE_HOST_KEY");
        assert!(refused.contains(&reason.text), "{refused}");
        assert!(!refused.contains("Edit account"), "{refused}");
        Ok(())
    })
    .await
}
