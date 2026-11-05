// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_console::backend::Effect;
use jackin_console::domain::account::{
    Account as ConsoleAccount, AccountForm, CredentialSource as ConsoleSource, DetectedKind,
    UNSAVED_ID,
};
use jackin_console::domain::agent::Provider as ConsoleProvider;
use jackin_console::domain::reason::{Found, IssueReason};
use jackin_console::domain::vocabulary::{Rejection, Vocabulary};
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
        let login = Vocabulary::openshell().found(&Found::Login {
            account: ACCOUNT_ID.to_owned(),
        });
        let shown: String = login.chars().take(40).collect();
        assert!(done.contains(&format!("✓ material   {shown}")), "{done}");
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
        let resolved = Vocabulary::openshell().found(&Found::ReferenceResolved);
        assert!(passed.contains(&format!("✓ material   {resolved}")), "{passed}");
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

async fn material_row(lab: &mut Lab, folder: &str) -> Result<String, Box<dyn std::error::Error>> {
    lab.reopen().await?;
    codex_folder_form(lab, folder).await?;
    lab.step("tab tab enter").await?;
    Ok(lab.console.screen())
}

#[tokio::test]
async fn a_failed_folder_check_is_worded_by_the_vocabulary_from_the_fault() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let words = Vocabulary::openshell();
        let broken = lab.code.join("broken");
        tokio::fs::create_dir_all(&broken).await?;
        tokio::fs::write(broken.join("auth.json"), "{").await?;
        let apikey = lab.code.join("apikey");
        tokio::fs::create_dir_all(&apikey).await?;
        tokio::fs::write(apikey.join("auth.json"), r#"{"auth_mode":"apikey"}"#).await?;
        let invalid = IssueReason::LoginInvalid {
            file: "auth.json".to_owned(),
        };
        let cases = [
            (
                lab.code.join("nowhere"),
                IssueReason::FolderMissing {
                    path: "nowhere".to_owned(),
                },
            ),
            (
                lab.code.join("alpha"),
                IssueReason::LoginMissing {
                    path: "alpha".to_owned(),
                    file: "auth.json".to_owned(),
                },
            ),
            (broken, invalid),
            (
                apikey,
                IssueReason::LoginMissing {
                    path: "apikey".to_owned(),
                    file: "ChatGPT login in auth.json".to_owned(),
                },
            ),
        ];
        for (folder, reason) in cases {
            let shown = material_row(&mut lab, &folder.display().to_string()).await?;
            let row = format!("! material   {}", words.check(&reason));
            assert!(shown.contains(&row), "{reason:?}: {shown}");
        }
        Ok(())
    })
    .await
}

async fn plant_login(lab: &Lab, folder: &Path, core: Provider) -> TestResult {
    tokio::fs::create_dir_all(folder).await?;
    let Some(file) = core.login_file() else {
        return Ok(());
    };
    let path = folder.join(file);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if core == Provider::Codex {
        tokio::fs::copy(lab.rig.home.as_path().join("auth.json"), path).await?;
    } else {
        tokio::fs::write(path, "login").await?;
    }
    Ok(())
}

fn assert_stored(
    label: &str,
    core: Provider,
    file: &str,
    name: &str,
    source: &CredentialSource,
) -> TestResult {
    match source {
        CredentialSource::CodexLogin { home, .. } => {
            assert_eq!(core, Provider::Codex, "{label}");
            assert!(home.as_path().ends_with(name), "{label}: {home}");
        }
        CredentialSource::File { path } => {
            assert!(path.as_path().ends_with(file), "{label}: {path}");
        }
        other => return Err(format!("{label}: {other:?}").into()),
    }
    Ok(())
}

#[tokio::test]
async fn the_form_lists_every_provider_and_saves_each_one_that_has_a_login_file() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let form = AccountForm::openshell();
        assert_eq!(form.providers.len(), Provider::ALL.len());
        for (index, choice) in form.providers.iter().enumerate() {
            let core = jackin_tui::core_provider(choice.provider);
            let name = format!("acct{index}");
            plant_login(&lab, &lab.code.join(&name), core).await?;
            lab.reopen().await?;
            let typed = format!("~/code/{name}");
            lab.folder_form(&name, index, &typed).await?;
            let filled = lab.console.screen();
            assert!(filled.contains(&choice.label), "{}: {filled}", choice.label);
            lab.save_form().await?;
            let config = lab.config().await?;
            let saved = config.accounts().find(|a| a.name().as_str() == name);
            let Some(file) = core.login_file() else {
                assert!(saved.is_none(), "{} has no login file", choice.label);
                let refused =
                    Vocabulary::openshell().rejection(&Rejection::NotAFolder { folder: typed });
                let screen = lab.console.screen();
                assert!(screen.contains(&refused), "{}: {screen}", choice.label);
                continue;
            };
            let account = saved.ok_or_else(|| format!("{} was not saved", choice.label))?;
            assert_eq!(account.provider(), core, "{}", choice.label);
            assert_stored(&choice.label, core, file, &name, account.credential())?;
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_folder_without_the_providers_login_file_is_refused_on_save() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let empty = lab.code.join("claude-empty");
        tokio::fs::create_dir_all(&empty).await?;
        let before = lab.console.config_text().await?;
        lab.folder_form("claudette", 0, "~/code/claude-empty")
            .await?;
        lab.save_form().await?;
        assert_eq!(lab.console.config_text().await?, before);
        let refused = Vocabulary::openshell().rejection(&Rejection::NotAFolder {
            folder: "~/code/claude-empty".to_owned(),
        });
        let screen = lab.console.screen();
        assert!(screen.contains(&refused), "{screen}");
        Ok(())
    })
    .await
}

async fn cli_file_account(
    lab: &Lab,
    name: &str,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let keys = lab.code.join("keys");
    tokio::fs::create_dir_all(&keys).await?;
    let file = keys.join(format!("{name}-login"));
    tokio::fs::write(&file, "login").await?;
    let account = jackin_core::Account::new(
        jackin_core::AccountName::parse(name)?,
        Provider::Claude,
        CredentialSource::File {
            path: jackin_core::HostPath::parse(&file)?,
        },
    )?;
    lab.edit(vec![jackin_core::ConfigEdit::AddAccount(account)])
        .await?;
    Ok(file)
}

async fn validate_edit_form(lab: &mut Lab) -> Result<String, Box<dyn std::error::Error>> {
    lab.step("c down down").await?;
    lab.step("e").await?;
    lab.step("tab tab tab tab").await?;
    lab.step("tab tab enter").await?;
    Ok(lab.console.screen())
}

#[tokio::test]
async fn a_cli_file_account_validates_as_a_file_and_a_file_path_saves_as_a_file() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let file = cli_file_account(&lab, "filed").await?;
        lab.reopen().await?;
        let before = lab.console.config_text().await?;
        let validated = validate_edit_form(&mut lab).await?;
        assert!(
            validated.contains("✓ material   File found at"),
            "{validated}"
        );
        assert!(!validated.contains("! material"), "{validated}");
        lab.step("tab tab enter").await?;
        assert_eq!(lab.console.config_text().await?, before);

        let other = file.with_file_name("other-login");
        tokio::fs::write(&other, "login").await?;
        let base = lab.world().accounts.clone();
        let mut registry = base.clone();
        let filed = registry
            .accounts
            .iter_mut()
            .find(|a| a.id == "filed")
            .ok_or("filed")?;
        filed.source = ConsoleSource::LocalFolder {
            path: other.display().to_string(),
            detected: DetectedKind::Unknown,
        };
        lab.perform(Effect::SaveAccounts {
            ticket: 1,
            base: Box::new(base),
            registry: Box::new(registry),
        })
        .await?;
        let config = lab.config().await?;
        let saved = config
            .account(&jackin_core::AccountName::parse("filed")?)
            .ok_or("filed")?;
        assert_eq!(
            *saved.credential(),
            CredentialSource::File {
                path: jackin_core::HostPath::parse(&other)?
            }
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cli_file_account_whose_file_is_gone_fails_the_check_and_the_save() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let file = cli_file_account(&lab, "filed").await?;
        lab.reopen().await?;
        let before = lab.console.config_text().await?;
        tokio::fs::remove_file(&file).await?;
        let validated = validate_edit_form(&mut lab).await?;
        assert!(validated.contains("! material"), "{validated}");
        assert!(!validated.contains("✓ material"), "{validated}");

        let base = lab.world().accounts.clone();
        let mut registry = base.clone();
        let filed = registry
            .accounts
            .iter_mut()
            .find(|a| a.id == "filed")
            .ok_or("filed")?;
        filed.source = ConsoleSource::LocalFolder {
            path: file.with_file_name("never-login").display().to_string(),
            detected: DetectedKind::Unknown,
        };
        lab.perform(Effect::SaveAccounts {
            ticket: 1,
            base: Box::new(base),
            registry: Box::new(registry),
        })
        .await?;
        assert_eq!(lab.console.config_text().await?, before);
        Ok(())
    })
    .await
}

async fn save_file_path(lab: &mut Lab, path: &Path) -> TestResult {
    let base = lab.world().accounts.clone();
    let mut registry = base.clone();
    let filed = registry
        .accounts
        .iter_mut()
        .find(|a| a.id == "filed")
        .ok_or("filed")?;
    filed.source = ConsoleSource::LocalFolder {
        path: path.display().to_string(),
        detected: DetectedKind::Unknown,
    };
    lab.perform(Effect::SaveAccounts {
        ticket: 1,
        base: Box::new(base),
        registry: Box::new(registry),
    })
    .await
}

#[tokio::test]
async fn a_symbolic_link_to_a_good_file_validates_and_saves_as_the_file_source() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let file = cli_file_account(&lab, "filed").await?;
        let real = file.with_file_name("real-login");
        tokio::fs::rename(&file, &real).await?;
        tokio::fs::symlink(&real, &file).await?;
        lab.reopen().await?;
        let validated = validate_edit_form(&mut lab).await?;
        assert!(
            validated.contains("✓ material   File found at"),
            "{validated}"
        );

        let other = file.with_file_name("other-login");
        tokio::fs::symlink(&real, &other).await?;
        save_file_path(&mut lab, &other).await?;
        let config = lab.config().await?;
        let saved = config
            .account(&jackin_core::AccountName::parse("filed")?)
            .ok_or("filed")?;
        assert_eq!(
            *saved.credential(),
            CredentialSource::File {
                path: jackin_core::HostPath::parse(&other)?
            }
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_oversized_file_or_a_link_to_a_pipe_fails_the_check_and_the_save() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let file = cli_file_account(&lab, "filed").await?;
        lab.reopen().await?;
        let before = lab.console.config_text().await?;
        tokio::fs::write(&file, vec![b'x'; (1 << 20) + 1]).await?;
        let validated = validate_edit_form(&mut lab).await?;
        assert!(validated.contains("! material"), "{validated}");
        assert!(!validated.contains("✓ material"), "{validated}");

        let big = file.with_file_name("big-login");
        tokio::fs::write(&big, vec![b'x'; (1 << 20) + 1]).await?;
        save_file_path(&mut lab, &big).await?;
        assert_eq!(lab.console.config_text().await?, before);

        let pipe = file.with_file_name("pipe-login");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&pipe)
                .status()?
                .success()
        );
        let linked = file.with_file_name("pipe-link");
        tokio::fs::symlink(&pipe, &linked).await?;
        save_file_path(&mut lab, &linked).await?;
        assert_eq!(lab.console.config_text().await?, before);
        Ok(())
    })
    .await
}

const CLAUDE_LOGIN: &str = ".credentials.json";

fn make_pipe(path: &Path) -> TestResult {
    assert!(
        std::process::Command::new("mkfifo")
            .arg(path)
            .status()?
            .success()
    );
    Ok(())
}

async fn save_claude_folder(lab: &mut Lab, name: &str) -> TestResult {
    lab.reopen().await?;
    lab.folder_form(name, 0, &format!("~/code/{name}")).await?;
    lab.save_form().await
}

#[tokio::test]
async fn a_claude_login_file_that_is_a_link_to_a_good_file_saves_as_the_login_file() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let folder = lab.code.join("linked");
        tokio::fs::create_dir_all(&folder).await?;
        let real = lab.code.join("real-login");
        tokio::fs::write(&real, "login").await?;
        tokio::fs::symlink(&real, folder.join(CLAUDE_LOGIN)).await?;
        save_claude_folder(&mut lab, "linked").await?;
        let config = lab.config().await?;
        let account = config
            .account(&jackin_core::AccountName::parse("linked")?)
            .ok_or("the account was not saved")?;
        assert_eq!(account.provider(), Provider::Claude);
        assert!(
            matches!(account.credential(), CredentialSource::File { path } if path.as_path().ends_with(format!("linked/{CLAUDE_LOGIN}"))),
            "{:?}",
            account.credential()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_oversized_claude_login_file_or_a_link_to_a_pipe_is_refused_and_the_config_stays()
-> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let before = lab.console.config_text().await?;
        let refused = Vocabulary::openshell().rejection(&Rejection::NotAFolder {
            folder: "~/code/big".to_owned(),
        });
        let big = lab.code.join("big");
        tokio::fs::create_dir_all(&big).await?;
        tokio::fs::write(big.join(CLAUDE_LOGIN), vec![b'x'; (1 << 20) + 1]).await?;
        save_claude_folder(&mut lab, "big").await?;
        let screen = lab.console.screen();
        assert!(screen.contains(&refused), "{screen}");
        assert_eq!(lab.console.config_text().await?, before);

        let piped = lab.code.join("piped");
        tokio::fs::create_dir_all(&piped).await?;
        let pipe = lab.code.join("login-pipe");
        make_pipe(&pipe)?;
        tokio::fs::symlink(&pipe, piped.join(CLAUDE_LOGIN)).await?;
        save_claude_folder(&mut lab, "piped").await?;
        assert_eq!(lab.console.config_text().await?, before);
        Ok(())
    })
    .await
}

async fn unreadable_codex_folders(
    lab: &Lab,
) -> Result<[std::path::PathBuf; 2], Box<dyn std::error::Error>> {
    let big = lab.code.join("codex-big");
    tokio::fs::create_dir_all(&big).await?;
    let mut login = tokio::fs::read(lab.rig.home.as_path().join("auth.json")).await?;
    login.resize((1 << 20) + 1, b' ');
    tokio::fs::write(big.join("auth.json"), login).await?;
    let piped = lab.code.join("codex-piped");
    tokio::fs::create_dir_all(&piped).await?;
    make_pipe(&piped.join("auth.json"))?;
    Ok([big, piped])
}

#[tokio::test]
async fn a_codex_folder_whose_auth_json_is_a_pipe_or_oversized_reads_as_unreadable_not_as_no_login()
-> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let words = Vocabulary::openshell();
        for folder in unreadable_codex_folders(&lab).await? {
            let shown = material_row(&mut lab, &folder.display().to_string()).await?;
            let canonical = tokio::fs::canonicalize(&folder)
                .await?
                .display()
                .to_string();
            let reason = IssueReason::FolderUnreadable {
                detail: format!("cannot read {canonical}"),
                path: canonical,
            };
            let expected: String = words.check(&reason).chars().take(40).collect();
            assert!(
                shown.contains(&format!("! material   {expected}")),
                "{shown}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_codex_folder_whose_auth_json_is_a_pipe_or_oversized_blocks_the_save() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let before = lab.console.config_text().await?;
        for folder in unreadable_codex_folders(&lab).await? {
            lab.reopen().await?;
            codex_folder_form(&mut lab, &folder.display().to_string()).await?;
            lab.step("tab tab tab tab enter").await?;
            assert_eq!(lab.console.config_text().await?, before);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn two_providers_on_one_folder_save_independently() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let shared = lab.code.join("shared");
        plant_login(&lab, &shared, Provider::Codex).await?;
        plant_login(&lab, &shared, Provider::Claude).await?;
        let folder = shared.display().to_string();
        let base = lab.world().accounts.clone();
        let mut registry = base.clone();
        for (name, provider) in [
            ("claude-shared", ConsoleProvider::Anthropic),
            ("codex-shared", ConsoleProvider::OpenAi),
        ] {
            registry.accounts.push(ConsoleAccount::registered(
                UNSAVED_ID,
                name,
                provider,
                ConsoleSource::LocalFolder {
                    path: folder.clone(),
                    detected: DetectedKind::Unknown,
                },
            ));
        }
        lab.perform(Effect::SaveAccounts {
            ticket: 1,
            base: Box::new(base),
            registry: Box::new(registry),
        })
        .await?;
        let config = lab.config().await?;
        let stored = |name: &str| -> Result<CredentialSource, Box<dyn std::error::Error>> {
            Ok(config
                .account(&jackin_core::AccountName::parse(name)?)
                .ok_or(format!("{name} was not saved"))?
                .credential()
                .clone())
        };
        let claude = stored("claude-shared")?;
        assert!(
            matches!(&claude, CredentialSource::File { path } if path.as_path().ends_with("shared/.credentials.json")),
            "{claude:?}"
        );
        let codex = stored("codex-shared")?;
        assert!(
            matches!(&codex, CredentialSource::CodexLogin { home, .. } if home.as_path().ends_with("shared")),
            "{codex:?}"
        );
        Ok(())
    })
    .await
}
