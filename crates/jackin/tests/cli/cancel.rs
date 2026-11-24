// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use jackin::ExitClass;
use jackin_core::{
    HookKind, NameSuffix, ProcessRunner, SandboxName, SandboxSlug, lock_path, sandbox_layout,
};
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_testkit::{
    CancelsOn, Fault, HostSnapshot, Rig, RoleFixture, TestResult, Trigger, guard, launch_id,
};
use tokio_util::sync::CancellationToken;

use super::support::{Failure, Harness, ScriptedTerminal, Step};

async fn rigged() -> Result<(Rig, Harness), Failure> {
    let rig = Rig::new().await?;
    let mut harness = Harness::new(&rig.world);
    harness.cwd = rig.repo.root().clone();
    harness.write_config(&rig.config.render())?;
    Ok((rig, harness))
}

#[tokio::test]
async fn a_cancelled_load_keeps_the_sandbox_names_it_and_a_rerun_completes() -> TestResult {
    guard(async {
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
        assert!(ran.stdout.is_empty());
        let name = rig.sandbox_name(1)?;
        let message = ran
            .stderr
            .lines()
            .find(|line| line.starts_with("cancelled"))
            .ok_or("no cancelled line")?;
        assert!(
            message.contains(name.as_str()) && message.contains(&id),
            "{}",
            ran.stderr
        );
        assert_eq!(rig.world.gateway.sandboxes().len(), 1);

        harness.run(&["load", "--launch-id", &id]).await?.ok()?;
        assert_eq!(rig.world.gateway.sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancelled_build_stores_no_image_and_the_same_build_uncancelled_succeeds() -> TestResult {
    guard(async {
        let (rig, harness) = rigged().await?;
        let token = CancellationToken::new();
        token.cancel();
        let ran = harness.command(&["build"]).cancel(token).run().await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stdout.is_empty());
        assert!(rig.world.engine.images().is_empty());

        harness.run(&["build"]).await?.ok()?;
        assert!(!rig.world.engine.images().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancelled_export_writes_no_ref_and_the_same_export_uncancelled_succeeds() -> TestResult {
    guard(async {
        let (rig, harness) = rigged().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        handle.git(&["checkout", "-q", "-b", "feature"]).await?;
        handle
            .commit_in_project("new.txt", b"new\n", "work")
            .await?;
        let name = handle.name();
        let token = CancellationToken::new();
        token.cancel();
        let ran = harness
            .command(&["export", name.as_str()])
            .cancel(token)
            .run()
            .await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(
            HostSnapshot::take(rig.repo.root())
                .await?
                .exported
                .is_empty()
        );

        harness.run(&["export", name.as_str()]).await?.ok()?;
        assert!(
            !HostSnapshot::take(rig.repo.root())
                .await?
                .exported
                .is_empty()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_restart_cancelled_after_preflight_exits_cancelled_and_leaves_the_start_request_alone()
-> TestResult {
    guard(async {
        let (rig, harness) = rigged().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        handle.exit_codex(2);
        let start = handle.file(&sandbox_layout::RUN_START);
        let token = CancellationToken::new();
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Program(sandbox_layout::hook_path(HookKind::Preflight)),
            token: token.clone(),
        });
        let (terminal, _) = ScriptedTerminal::new(vec![Step::Bytes(b"\x10\x11".to_vec())])?;
        let name = handle.name();
        let ran = harness
            .command(&["attach", name.as_str(), "--restart"])
            .terminal(terminal)
            .cancel(token)
            .run()
            .await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert_eq!(handle.file(&sandbox_layout::RUN_START), start);
        Ok(())
    })
    .await
}

async fn git_in(dir: &Path, args: &[&str]) -> Result<String, Failure> {
    let output = tokio::process::Command::new("/usr/bin/git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .await?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

#[tokio::test]
async fn a_cancel_during_the_role_fetch_ends_load_and_build_and_leaves_the_cache_at_the_old_commit()
-> TestResult {
    guard(async {
        for command in ["load", "build"] {
            let (rig, harness) = rigged().await?;
            let origin = rig.loaded.root().as_path().to_path_buf();
            let url = "https://example.invalid/role.git";
            std::fs::write(
                rig.world.home().as_path().join(".gitconfig"),
                format!(
                    "[url \"file://{}\"]\n\tinsteadOf = {url}\n",
                    origin.display()
                ),
            )?;
            let path_source = format!("source = {{ path = \"{}\" }}", origin.display());
            let config = rig.config.render();
            assert!(config.contains(&path_source));
            harness.write_config(
                &config.replace(&path_source, &format!("source = {{ git = \"{url}\" }}")),
            )?;
            let first = harness.run(&[command]).await?;
            assert_eq!(first.exit, ExitClass::Ok, "{}", first.stderr);
            let cached = rig.world.cache_dir().as_path().join("roles/sentinel");
            let old = git_in(&cached, &["rev-parse", "HEAD"]).await?;

            git_in(&origin, &["commit", "-q", "--allow-empty", "-m", "advance"]).await?;
            let token = CancellationToken::new();
            let world_runner = Arc::clone(&rig.world.runner);
            let cancelling: Arc<dyn ProcessRunner> =
                Arc::new(CancelsOn::new(&world_runner, "fetch", &token));
            let ran = harness
                .runner(cancelling)
                .command(&[command])
                .cancel(token)
                .run()
                .await?;
            assert_eq!(ran.exit, ExitClass::Cancelled, "{command}: {}", ran.stderr);
            assert_eq!(
                git_in(&cached, &["rev-parse", "HEAD"]).await?,
                old,
                "{command}"
            );

            let again = harness.run(&[command]).await?;
            assert_eq!(again.exit, ExitClass::Ok, "{command}: {}", again.stderr);
            assert_ne!(
                git_in(&cached, &["rev-parse", "HEAD"]).await?,
                old,
                "{command}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn cancelling_load_while_it_waits_for_an_environment_value_ends_the_command() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?.env("TOKENISH", None, None);
        let rig = Rig::builder().role(role).build().await?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.write_config(&rig.config.render())?;
        let (terminal, probe) = ScriptedTerminal::new(Vec::new())?;
        let token = CancellationToken::new();
        let running = harness
            .command(&["load"])
            .terminal(terminal)
            .cancel(token.clone())
            .run();
        let driver = async {
            probe.pending_read().await;
            token.cancel();
        };
        let (ran, ()) = tokio::join!(running, driver);
        let ran = ran?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stdout.is_empty());
        assert!(rig.world.gateway.sandboxes().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}

fn cancelled_line(stderr: &str) -> Result<&str, Failure> {
    Ok(stderr
        .lines()
        .find(|line| line.starts_with("cancelled"))
        .ok_or("no cancelled line")?)
}

fn lost_create() -> GatewayError {
    GatewayError::Timeout {
        op: GatewayOp::Create,
        after: std::time::Duration::from_secs(30),
        group: None,
    }
}

#[tokio::test]
async fn a_cancelled_load_says_a_sandbox_never_existed_only_when_none_was_requested() -> TestResult
{
    guard(async {
        let (rig, harness) = rigged().await?;
        let token = CancellationToken::new();
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Provider),
            token: token.clone(),
        });
        let id = launch_id(1).to_string();
        let ran = harness
            .command(&["load", "--launch-id", &id])
            .cancel(token)
            .run()
            .await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        let line = cancelled_line(&ran.stderr)?;
        assert!(
            line.contains("before a sandbox existed") && line.contains(&id),
            "{line}"
        );
        assert!(rig.gateway().sandboxes().is_empty());

        let (rig, harness) = rigged().await?;
        let token = CancellationToken::new();
        rig.gateway().inject(Fault::ProvisionFails { creates: 1 });
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Delete),
            token: token.clone(),
        });
        let ran = harness
            .command(&["load", "--launch-id", &id])
            .cancel(token)
            .run()
            .await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        let line = cancelled_line(&ran.stderr)?;
        assert!(
            line.contains("no sandbox exists") && !line.contains("before a sandbox existed"),
            "{line}"
        );
        assert!(rig.gateway().sandboxes().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancelled_load_never_says_no_sandbox_when_the_gateway_cannot_confirm() -> TestResult {
    guard(async {
        let id = launch_id(1).to_string();

        let (rig, harness) = rigged().await?;
        let token = CancellationToken::new();
        rig.gateway()
            .inject(Fault::HideEverywhereUntilCreateConflict);
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Create),
            token: token.clone(),
        });
        rig.gateway().inject(Fault::LoseResponse {
            op: GatewayOp::Create,
            error: lost_create(),
        });
        let ran = harness
            .command(&["load", "--launch-id", &id])
            .cancel(token)
            .run()
            .await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        let line = cancelled_line(&ran.stderr)?;
        let planned = rig.sandbox_name(1)?;
        assert!(
            line.contains(&format!(
                "could not confirm whether sandbox {planned} exists"
            ))
                && !line.contains("before a sandbox existed")
                && line.contains(&id),
            "{line}"
        );
        assert_eq!(rig.world.gateway.sandboxes().len(), 1);
        harness.run(&["load", "--launch-id", &id]).await?.ok()?;
        assert_eq!(rig.world.gateway.sandboxes().len(), 1);

        let (rig, harness) = rigged().await?;
        let hold = Trigger::Op(GatewayOp::UploadTree);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let running = harness
            .command(&["load", "--launch-id", &id])
            .cancel(token.clone())
            .run();
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            rig.gateway().inject(Fault::FailNext {
                op: GatewayOp::Find,
                error: GatewayError::Unreachable {
                    endpoint: "fake".to_owned(),
                    message: "down".to_owned(),
                },
            });
            token.cancel();
            rig.gateway().release(&hold);
        };
        let (ran, ()) = tokio::join!(running, driver);
        let ran = ran?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        let line = cancelled_line(&ran.stderr)?;
        let name = rig.sandbox_name(1)?;
        assert!(
            line.contains(name.as_str())
                && line.contains("was seen during this launch")
                && !line.contains("before a sandbox existed"),
            "{line}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancelled_load_with_several_matching_sandboxes_names_them_all() -> TestResult {
    guard(async {
        let (rig, harness) = rigged().await?;
        let id = launch_id(1).to_string();
        let hold = Trigger::Op(GatewayOp::UploadTree);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let extra = SandboxName::new(&SandboxSlug::parse("dup")?, &NameSuffix::parse("bbbb")?);
        let running = harness
            .command(&["load", "--launch-id", &id])
            .cancel(token.clone())
            .run();
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            rig.foreign_sandbox(extra.clone(), 1).await?;
            token.cancel();
            rig.gateway().release(&hold);
            Ok::<(), Failure>(())
        };
        let (ran, driven) = tokio::join!(running, driver);
        driven?;
        let ran = ran?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        let line = cancelled_line(&ran.stderr)?;
        let ours = rig.sandbox_name(1)?;
        assert!(
            line.contains(ours.as_str())
                && line.contains(extra.as_str())
                && !line.contains("before a sandbox existed")
                && !line.contains("remains as a restore candidate"),
            "{line}"
        );
        assert_eq!(rig.world.gateway.sandboxes().len(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_while_the_config_lock_is_held_exits_cancelled_and_changes_nothing() -> TestResult
{
    guard(async {
        let (rig, harness) = rigged().await?;
        let before = harness.config_bytes()?;
        let held = rig.world.fs.lock(&lock_path(&harness.config_path)?).await?;
        let token = CancellationToken::new();
        let elsewhere = rig.home.as_path().display().to_string();
        let running = harness
            .command(&["workspace", "add", "extra", &elsewhere])
            .cancel(token.clone())
            .run();
        let canceller = async {
            while rig.world.clock.slept().is_empty() {
                tokio::task::yield_now().await;
            }
            token.cancel();
        };
        let (ran, ()) = tokio::join!(running, canceller);
        let ran = ran?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stdout.is_empty());
        assert_eq!(harness.config_bytes()?, before);

        drop(held);
        harness
            .run(&["workspace", "add", "extra", &elsewhere])
            .await?
            .ok()?;
        assert_ne!(harness.config_bytes()?, before);
        Ok(())
    })
    .await
}
