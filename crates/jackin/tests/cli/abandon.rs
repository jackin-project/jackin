// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin::ExitClass;
use jackin_core::{AccountName, Agent};
use jackin_openshell::GatewayOp;
use jackin_testkit::{Fault, Rig, RoleFixture, TestResult, Trigger, guard_within, launch_id};
use tokio_util::sync::CancellationToken;

use super::support::{Failure, Harness, ScriptedTerminal, Step};

const WITHIN: Duration = Duration::from_secs(40);

async fn with_env() -> Result<(Rig, Harness), Failure> {
    let role = RoleFixture::new("sentinel").map(|role| role.env("TOKENISH", None, None))?;
    let rig = Rig::builder().role(role).build().await?;
    let mut harness = Harness::new(&rig.world);
    harness.cwd = rig.repo.root().clone();
    harness.write_config(&rig.config.render())?;
    Ok((rig, harness))
}

async fn rigged() -> Result<(Rig, Harness), Failure> {
    let rig = Rig::new().await?;
    let mut harness = Harness::new(&rig.world);
    harness.cwd = rig.repo.root().clone();
    harness.write_config(&rig.config.render())?;
    Ok((rig, harness))
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
        assert!(ran.stderr.contains("interrupted again"), "{}", ran.stderr);
        assert!(ran.stderr.contains("not confirmed"), "{}", ran.stderr);
        assert!(ran.stderr.contains(name.as_str()), "{}", ran.stderr);
        assert!(
            ran.stderr.contains("delete may have been requested"),
            "{}",
            ran.stderr
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_second_interrupt_during_a_load_never_prints_an_env_value() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, harness) = with_env().await?;
        let hold = Trigger::Op(GatewayOp::Create);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let id = launch_id(1).to_string();
        let abandon = CancellationToken::new();
        let command = harness
            .command(&[
                "load",
                "--launch-id",
                &id,
                "--env",
                "TOKENISH=unique-env-value-7c1f",
            ])
            .abandon(abandon.clone());
        let (ran, ()) = tokio::try_join!(command.run(), async {
            assert!(rig.gateway().until_held(&hold).await);
            abandon.cancel();
            Ok::<_, Failure>(())
        })?;
        rig.gateway().release(&hold);

        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stderr.contains("interrupted again"), "{}", ran.stderr);
        assert!(ran.stderr.contains("not confirmed"), "{}", ran.stderr);
        assert!(
            ran.stderr.contains(&format!("--launch-id {id}")),
            "{}",
            ran.stderr
        );
        assert!(ran.stderr.contains("sandbox jackin-"), "{}", ran.stderr);
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
async fn a_second_interrupt_during_a_load_names_the_launch_and_the_planned_sandbox() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, harness) = rigged().await?;
        let hold = Trigger::Op(GatewayOp::Create);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let id = launch_id(1).to_string();
        let abandon = CancellationToken::new();
        let command = harness
            .command(&["load", "--launch-id", &id])
            .abandon(abandon.clone());
        let (ran, ()) = tokio::try_join!(command.run(), async {
            assert!(rig.gateway().until_held(&hold).await);
            abandon.cancel();
            Ok::<_, Failure>(())
        })?;
        rig.gateway().release(&hold);

        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stderr.contains("not confirmed"), "{}", ran.stderr);
        assert!(
            ran.stderr.contains(&format!("--launch-id {id}")),
            "{}",
            ran.stderr
        );
        assert!(
            ran.stderr.contains(rig.sandbox_name(1)?.as_str()),
            "{}",
            ran.stderr
        );
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
        let (terminal, raw) = ScriptedTerminal::new(vec![Step::Line(String::new())])?;
        let abandon = CancellationToken::new();
        let command = harness
            .command(&["load", "--launch-id", &id])
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
        assert!(!raw.raw());
        assert!(ran.stderr.contains("interrupted again"), "{}", ran.stderr);
        assert!(!ran.stderr.contains("may exist"), "{}", ran.stderr);
        assert!(!ran.stderr.contains("--launch-id"), "{}", ran.stderr);
        assert_eq!(rig.world.gateway.sandboxes().len(), 1);
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
        assert!(ran.stderr.contains("cancelled"), "{}", ran.stderr);
        assert!(!ran.stderr.contains("interrupted again"), "{}", ran.stderr);
        assert!(!ran.stderr.contains("not confirmed"), "{}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_second_interrupt_restores_the_terminal_of_an_attached_session() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, harness) = rigged().await?;
        rig.launch(1).await?;
        let name = rig.sandbox_name(1)?;
        let (terminal, raw) = ScriptedTerminal::new(vec![])?;
        let abandon = CancellationToken::new();
        let command = harness
            .command(&["attach", name.as_str()])
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
        assert!(!raw.raw());
        assert!(ran.stderr.contains("interrupted again"), "{}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_second_interrupt_during_an_account_edit_says_provider_state_may_be_partly_changed()
-> TestResult {
    guard_within(WITHIN, async {
        let rig = Rig::new().await?;
        let harness = Harness::new(&rig.world);
        harness.write_config(&rig.config.render())?;
        let work = AccountName::parse("work")?;
        let account = rig.config.account(&work).ok_or("no work account")?;
        rig.world
            .session
            .sync_account(account, Agent::Codex, &CancellationToken::new())
            .await?;
        let hold = Trigger::Op(GatewayOp::Provider);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let abandon = CancellationToken::new();
        let command = harness
            .command(&["account", "rm", "work"])
            .abandon(abandon.clone());
        let (ran, ()) = tokio::try_join!(command.run(), async {
            assert!(rig.gateway().until_held(&hold).await);
            abandon.cancel();
            Ok::<_, Failure>(())
        })?;
        rig.gateway().release(&hold);

        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stderr.contains("interrupted again"), "{}", ran.stderr);
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
        let hold = Trigger::Op(GatewayOp::Find);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let abandon = CancellationToken::new();
        let command = harness.command(&["ls"]).abandon(abandon.clone());
        let (ran, ()) = tokio::try_join!(command.run(), async {
            assert!(rig.gateway().until_held(&hold).await);
            abandon.cancel();
            Ok::<_, Failure>(())
        })?;
        rig.gateway().release(&hold);

        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stderr.contains("interrupted again"), "{}", ran.stderr);
        assert!(
            ran.stderr.contains("may or may not be complete"),
            "{}",
            ran.stderr
        );
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
        let hold = Trigger::Op(GatewayOp::Program);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let abandon = CancellationToken::new();
        let command = harness
            .command(&["export", name.as_str()])
            .abandon(abandon.clone());
        let (ran, ()) = tokio::try_join!(command.run(), async {
            assert!(rig.gateway().until_held(&hold).await);
            abandon.cancel();
            Ok::<_, Failure>(())
        })?;
        rig.gateway().release(&hold);

        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(
            ran.stderr
                .contains("the export may have written refs and files on the host"),
            "{}",
            ran.stderr
        );
        assert!(ran.stderr.contains(name.as_str()), "{}", ran.stderr);
        Ok(())
    })
    .await
}
