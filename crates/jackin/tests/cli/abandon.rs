// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin::ExitClass;
use jackin_core::{AccountName, Agent};
use jackin_openshell::GatewayOp;
use jackin_testkit::{Fault, Rig, RoleFixture, TestResult, Trigger, guard_within, launch_id};
use tokio_util::sync::CancellationToken;

use super::support::{Failure, Harness, Ran, ScriptedTerminal, Step};

const WITHIN: Duration = Duration::from_secs(40);

async fn rigged() -> Result<(Rig, Harness), Failure> {
    harnessed(Rig::new().await?)
}

async fn with_env() -> Result<(Rig, Harness), Failure> {
    let role = RoleFixture::new("sentinel").map(|role| role.env("TOKENISH", None, None))?;
    harnessed(Rig::builder().role(role).build().await?)
}

fn harnessed(rig: Rig) -> Result<(Rig, Harness), Failure> {
    let mut harness = Harness::new(&rig.world);
    harness.cwd = rig.repo.root().clone();
    harness.write_config(&rig.config.render())?;
    Ok((rig, harness))
}

async fn abandoned_while_held(
    rig: &Rig,
    harness: &Harness,
    args: &[&str],
    op: GatewayOp,
) -> Result<Ran, Failure> {
    let hold = Trigger::Op(op);
    rig.gateway().inject(Fault::HoldAt {
        trigger: hold.clone(),
    });
    let abandon = CancellationToken::new();
    let command = harness.command(args).abandon(abandon.clone());
    let (ran, ()) = tokio::try_join!(command.run(), async {
        assert!(rig.gateway().until_held(&hold).await);
        abandon.cancel();
        Ok::<_, Failure>(())
    })?;
    rig.gateway().release(&hold);
    assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
    assert!(ran.stderr.contains("interrupted again"), "{}", ran.stderr);
    Ok(ran)
}

fn says(ran: &Ran, wanted: &[&str]) {
    for text in wanted {
        assert!(ran.stderr.contains(text), "{text}: {}", ran.stderr);
    }
}

#[tokio::test]
async fn a_second_interrupt_reports_that_a_purge_in_flight_is_not_confirmed() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, harness) = rigged().await?;
        rig.launch(1).await?;
        let name = rig.sandbox_name(1)?;
        let hold = Trigger::Op(GatewayOp::Delete);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let (cancel, abandon) = (CancellationToken::new(), CancellationToken::new());
        let command = harness
            .command(&["purge", name.as_str(), "--force"])
            .cancel(cancel.clone())
            .abandon(abandon.clone());
        let (ran, ()) = tokio::try_join!(command.run(), async {
            assert!(rig.gateway().until_held(&hold).await);
            cancel.cancel();
            tokio::task::yield_now().await;
            assert!(!abandon.is_cancelled());
            abandon.cancel();
            Ok::<_, Failure>(())
        })?;
        rig.gateway().release(&hold);

        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stdout.is_empty());
        says(
            &ran,
            &[
                "interrupted again",
                "not confirmed",
                name.as_str(),
                "delete may have been requested",
            ],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_second_interrupt_during_a_load_names_the_launch_and_the_planned_sandbox() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, harness) = rigged().await?;
        let id = launch_id(1).to_string();
        let ran = abandoned_while_held(
            &rig,
            &harness,
            &["load", "--launch-id", &id],
            GatewayOp::Create,
        )
        .await?;
        says(
            &ran,
            &[
                "not confirmed",
                &format!("--launch-id {id}"),
                rig.sandbox_name(1)?.as_str(),
            ],
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_second_interrupt_during_a_load_never_prints_an_env_value() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, harness) = with_env().await?;
        let id = launch_id(1).to_string();
        let ran = abandoned_while_held(
            &rig,
            &harness,
            &[
                "load",
                "--launch-id",
                &id,
                "--env",
                "TOKENISH=unique-env-value-7c1f",
            ],
            GatewayOp::Create,
        )
        .await?;
        says(
            &ran,
            &[
                "not confirmed",
                &format!("--launch-id {id}"),
                "sandbox jackin-",
            ],
        );
        assert!(
            !ran.stderr.contains("unique-env-value-7c1f"),
            "{}",
            ran.stderr
        );
        assert!(
            !ran.stdout.contains("unique-env-value-7c1f"),
            "{}",
            ran.stdout
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_single_interrupt_that_the_command_handles_prints_no_abandon_report() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, harness) = rigged().await?;
        let token = CancellationToken::new();
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::UploadTree),
            token: token.clone(),
        });
        let id = launch_id(1).to_string();
        let ran = harness
            .command(&["load", "--launch-id", &id])
            .cancel(token)
            .run()
            .await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        says(&ran, &["cancelled"]);
        assert!(!ran.stderr.contains("interrupted again"), "{}", ran.stderr);
        assert!(!ran.stderr.contains("not confirmed"), "{}", ran.stderr);
        Ok(())
    })
    .await
}

async fn abandoned_while_attached(
    harness: &Harness,
    args: &[&str],
) -> Result<(Ran, bool), Failure> {
    let (terminal, raw) = ScriptedTerminal::new(vec![Step::Line(String::new())])?;
    let abandon = CancellationToken::new();
    let command = harness
        .command(args)
        .terminal(terminal)
        .abandon(abandon.clone());
    let (ran, ()) = tokio::try_join!(command.run(), async {
        while !raw.raw() {
            tokio::task::yield_now().await;
        }
        abandon.cancel();
        Ok::<_, Failure>(())
    })?;
    assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
    assert!(ran.stderr.contains("interrupted again"), "{}", ran.stderr);
    Ok((ran, raw.raw()))
}

#[tokio::test]
async fn a_second_interrupt_restores_the_terminal_of_an_attached_session() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, harness) = rigged().await?;
        rig.launch(1).await?;
        let name = rig.sandbox_name(1)?;
        let (_, still_raw) = abandoned_while_attached(&harness, &["attach", name.as_str()]).await?;
        assert!(!still_raw);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_second_interrupt_while_attached_after_a_load_does_not_say_the_sandbox_may_not_exist()
-> TestResult {
    guard_within(WITHIN, async {
        let (rig, harness) = rigged().await?;
        let id = launch_id(1).to_string();
        let (ran, still_raw) =
            abandoned_while_attached(&harness, &["load", "--launch-id", &id]).await?;
        assert!(!still_raw);
        assert!(!ran.stderr.contains("may exist"), "{}", ran.stderr);
        assert!(!ran.stderr.contains("--launch-id"), "{}", ran.stderr);
        assert_eq!(rig.world.gateway.sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_second_interrupt_during_an_account_edit_says_provider_state_may_be_partly_changed()
-> TestResult {
    guard_within(WITHIN, async {
        let (rig, harness) = rigged().await?;
        let work = AccountName::parse("work")?;
        let account = rig.config.account(&work).ok_or("no work account")?;
        rig.world
            .session
            .sync_account(account, Agent::Codex, &CancellationToken::new())
            .await?;
        let ran = abandoned_while_held(
            &rig,
            &harness,
            &["account", "rm", "work"],
            GatewayOp::Provider,
        )
        .await?;
        assert!(
            ran.stderr.contains("provider state") && ran.stderr.contains("partly"),
            "{}",
            ran.stderr
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_second_interrupt_during_a_command_that_records_nothing_gives_a_neutral_line()
-> TestResult {
    guard_within(WITHIN, async {
        let (rig, harness) = rigged().await?;
        let ran = abandoned_while_held(&rig, &harness, &["ls"], GatewayOp::Find).await?;
        says(&ran, &["may or may not be complete"]);
        assert!(!ran.stderr.contains("may exist"), "{}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_second_interrupt_during_an_export_says_the_host_may_hold_part_of_it() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, harness) = rigged().await?;
        rig.launch(1).await?;
        let name = rig.sandbox_name(1)?;
        let ran = abandoned_while_held(
            &rig,
            &harness,
            &["export", name.as_str()],
            GatewayOp::Program,
        )
        .await?;
        says(
            &ran,
            &[
                "the export may have written refs and files on the host",
                name.as_str(),
            ],
        );
        Ok(())
    })
    .await
}
