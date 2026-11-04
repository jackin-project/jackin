// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::domain::account::CredentialSource as ConsoleSource;
use jackin_console::domain::reason::{Found, IssueReason};
use jackin_console::domain::vocabulary::Vocabulary;
use jackin_core::{CredentialSource, Provider};
use jackin_testkit::{Rig, TestResult, guard};

use super::support::Lab;

const VAR: &str = "JBG6_FAKE_KEY";
const VALUE: &str = "fake-value-0000";
const EMPTY: &str = "JBG6_EMPTY_KEY";
const UNSET: &str = "JBG6_UNSET_KEY";
const PASTED: &str = "sk-FAKE pasted 99";

async fn lab() -> Result<Lab, Box<dyn std::error::Error>> {
    Lab::with_env(Rig::new().await?, &[(VAR, VALUE), (EMPTY, "  ")]).await
}

async fn env_form(lab: &mut Lab, account: &str, typed: &str) -> TestResult {
    lab.open_form(account).await?;
    lab.step("tab down down space tab enter").await?;
    lab.console.type_text(typed);
    lab.console.settle().await?;
    lab.step("enter").await
}

async fn validate(lab: &mut Lab) -> Result<String, Box<dyn std::error::Error>> {
    lab.step("tab enter").await?;
    Ok(lab.console.screen())
}

#[tokio::test]
async fn a_set_variable_validates_and_saving_stores_only_its_name() -> TestResult {
    guard(async {
        let mut lab = lab().await?;
        env_form(&mut lab, "envy", VAR).await?;
        let checked = validate(&mut lab).await?;
        let set = Vocabulary::openshell().found(&Found::EnvSet {
            name: VAR.to_owned(),
        });
        assert!(
            checked.contains(&format!("✓ material   {set}")),
            "{checked}"
        );
        assert!(!checked.contains(VALUE), "{checked}");
        lab.step("tab tab enter").await?;
        let config = lab.config().await?;
        let saved = config
            .accounts()
            .find(|account| account.name().as_str() == "envy")
            .ok_or("the account was not saved")?;
        assert_eq!(saved.provider(), Provider::Claude);
        assert!(
            matches!(saved.credential(), CredentialSource::Env { var } if var.as_str() == VAR),
            "{:?}",
            saved.credential()
        );
        let text = lab.console.config_text().await?;
        assert!(text.contains(&format!("var = \"{VAR}\"")), "{text}");
        assert!(!text.contains(VALUE), "{text}");
        let screen = lab.console.screen();
        assert!(!screen.contains(VALUE), "{screen}");
        let registry = lab.world().accounts.clone();
        let account = registry.get("envy").ok_or("envy is not on the roster")?;
        assert!(
            matches!(&account.source, ConsoleSource::HostEnv { var, .. } if var == VAR),
            "{:?}",
            account.source
        );
        assert!(!format!("{registry:#?}").contains(VALUE));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unset_and_an_empty_variable_fail_the_check_and_block_the_save() -> TestResult {
    guard(async {
        let words = Vocabulary::openshell();
        for (name, reason) in [
            (
                UNSET,
                IssueReason::EnvMissing {
                    name: UNSET.to_owned(),
                },
            ),
            (
                EMPTY,
                IssueReason::EnvEmpty {
                    name: EMPTY.to_owned(),
                },
            ),
        ] {
            let mut lab = lab().await?;
            let before = lab.console.config_text().await?;
            env_form(&mut lab, "envy", name).await?;
            let failed = validate(&mut lab).await?;
            let row = format!("! material   {}", words.check(&reason));
            assert!(failed.contains(&row), "{reason:?}: {failed}");
            assert!(!failed.contains("✓ material"), "{reason:?}: {failed}");
            lab.step("tab tab enter").await?;
            assert_eq!(lab.console.config_text().await?, before, "{reason:?}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_invalid_name_fails_the_check_and_the_screen_shows_it_only_in_the_field() -> TestResult {
    guard(async {
        let mut lab = lab().await?;
        let before = lab.console.config_text().await?;
        env_form(&mut lab, "envy", PASTED).await?;
        let failed = validate(&mut lab).await?;
        let reason = IssueReason::EnvInvalid {
            name: PASTED.to_owned(),
        };
        let row = format!("! material   {}", Vocabulary::openshell().check(&reason));
        assert!(failed.contains(&row), "{failed}");
        assert_eq!(failed.matches(PASTED).count(), 1, "{failed}");
        lab.step("tab tab enter").await?;
        let blocked = lab.console.screen();
        assert_eq!(blocked.matches(PASTED).count(), 1, "{blocked}");
        let after = lab.console.config_text().await?;
        assert_eq!(after, before);
        assert!(!after.contains(PASTED));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_variable_another_account_of_the_provider_reads_is_a_duplicate() -> TestResult {
    guard(async {
        let mut lab = lab().await?;
        env_form(&mut lab, "envy", VAR).await?;
        lab.step("tab tab tab enter").await?;
        assert!(
            lab.config()
                .await?
                .accounts()
                .any(|a| a.name().as_str() == "envy")
        );
        let before = lab.console.config_text().await?;
        env_form(&mut lab, "envy2", VAR).await?;
        lab.step("tab tab tab enter").await?;
        let refused = lab.console.screen();
        assert!(
            refused.contains("Already registered: this source is used by Claude · envy"),
            "{refused}"
        );
        assert_eq!(lab.console.config_text().await?, before);
        Ok(())
    })
    .await
}
