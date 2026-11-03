// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_session::CheckId;
use jackin_testkit::{ProcessReply, ScriptedProgram, TestResult, World, guard};
use tokio_util::sync::CancellationToken;

use super::support::{Failure, FixedConnector, GATEWAY_CONFIG, Harness, Ran, connecting};

const IDS: [&str; 7] = [
    "openshell-binary",
    "ssh-binary",
    "git-binary",
    "gateway-registered",
    "gateway-health",
    "pinned-version",
    "engine-socket",
];

fn reported(ran: &Ran, class: ExitClass, ids: &[&str], stderr: &[&str]) -> Result<(), Failure> {
    if ran.exit != class {
        return Err(format!(
            "exit {:?}, wanted {class:?}; stderr: {}",
            ran.exit, ran.stderr
        )
        .into());
    }
    for id in ids {
        if ran.stdout.matches(id).count() != 1 || ran.stderr.contains(id) {
            return Err(format!(
                "{id} is not one stdout line: {} / {}",
                ran.stdout, ran.stderr
            )
            .into());
        }
    }
    for text in stderr {
        if !ran.stderr.contains(text) {
            return Err(format!("stderr lacks {text}: {}", ran.stderr).into());
        }
    }
    Ok(())
}

struct Fixture {
    world: World,
    harness: Harness,
}

fn fixture(socket: bool, openshell: ProcessReply) -> Result<Fixture, Failure> {
    let world = World::builder().build()?;
    let mut harness = Harness::new(&world);
    let scripted = world.runner.scripted();
    scripted.expect(ScriptedProgram::Openshell, &["--version"], openshell);
    scripted.expect(
        ScriptedProgram::Ssh,
        &["-V"],
        ProcessReply::exited(0, b"", b"OpenSSH_9.9p1\n"),
    );
    scripted.expect_git_version(ProcessReply::exited(
        0,
        b"git version 2.50.1 (Apple Git-155)\n",
        b"",
    ));
    scripted.expect(
        ScriptedProgram::Openshell,
        &["gateway", "list", "-o", "json"],
        ProcessReply::exited(
            0,
            b"[{\"name\":\"fake-gateway\",\"endpoint\":\"https://127.0.0.1:8080\",\"auth\":\"mtls\"}]",
            b"",
        ),
    );
    let socket_path = world.home().as_path().join("docker.sock");
    if socket {
        std::fs::write(&socket_path, b"")?;
    }
    harness.env.push((
        "DOCKER_HOST".to_owned(),
        format!("unix://{}", socket_path.display()),
    ));
    harness.write_config(GATEWAY_CONFIG)?;
    Ok(Fixture { world, harness })
}

fn healthy() -> Result<Fixture, Failure> {
    fixture(true, ProcessReply::exited(0, b"openshell 0.1.6\n", b""))
}

#[tokio::test]
async fn healthy_fakes_exit_ok_and_print_one_line_per_check_id_on_stdout_only() -> TestResult {
    guard(async {
        let Fixture {
            harness,
            world: _world,
        } = healthy()?;
        let ran = harness.run(&["doctor"]).await?;
        ran.ok()?;
        assert!(ran.stderr.is_empty(), "{}", ran.stderr);
        for line in ran.stdout.lines().filter(|line| line.starts_with("pass")) {
            assert!(!line.contains("fix:"), "{line}");
        }
        reported(&ran, ExitClass::Ok, &IDS, &[])
    })
    .await
}

#[tokio::test]
async fn a_missing_engine_socket_is_only_a_warning() -> TestResult {
    guard(async {
        let Fixture {
            harness,
            world: _world,
        } = fixture(false, ProcessReply::exited(0, b"openshell 0.1.6\n", b""))?;
        let ran = harness.run(&["doctor"]).await?;
        ran.ok()?;
        assert!(ran.stderr.is_empty(), "{}", ran.stderr);
        assert!(ran.stdout.contains("warn"), "{}", ran.stdout);
        assert_eq!(
            ran.stdout.matches("fix:").count(),
            1,
            "only the warning carries a fix: {}",
            ran.stdout
        );
        assert!(
            ran.stdout
                .contains(&format!("fix: {}", CheckId::EngineSocket.fix())),
            "{}",
            ran.stdout
        );
        reported(&ran, ExitClass::Ok, &IDS, &[])
    })
    .await
}

#[tokio::test]
async fn a_failed_check_fails_the_run_names_its_id_and_never_hides_the_others() -> TestResult {
    guard(async {
        let Fixture { world, harness } = healthy()?;
        world.gateway.set_serving(false);
        let ran = harness.run(&["doctor"]).await?;
        reported(&ran, ExitClass::Failure, &IDS, &["error:"])?;
        let health = ran
            .stdout
            .lines()
            .find(|line| line.contains("gateway-health"))
            .ok_or("no gateway-health line")?;
        assert!(
            health.ends_with(&format!("fix: {}", CheckId::GatewayHealth.fix())),
            "{health}"
        );

        let Fixture {
            mut harness,
            world: _kept,
        } = healthy()?;
        harness.connector = FixedConnector::empty();
        let ran = harness.run(&["doctor"]).await?;
        reported(&ran, ExitClass::Failure, &IDS, &["error:"])?;
        assert!(ran.stdout.contains("fail"), "{}", ran.stdout);

        let Fixture {
            harness,
            world: _world,
        } = fixture(true, ProcessReply::spawn("not found"))?;
        let ran = harness.run(&["doctor"]).await?;
        reported(
            &ran,
            ExitClass::Failure,
            &["openshell-binary", "ssh-binary", "engine-socket"],
            &["error:"],
        )?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn doctor_without_a_gateway_fails_and_names_the_command_that_adds_one() -> TestResult {
    guard(async {
        let Fixture {
            harness,
            world: _world,
        } = healthy()?;
        harness.write_config("")?;
        let ran = harness.run(&["doctor"]).await?;
        reported(
            &ran,
            ExitClass::Failure,
            &["openshell-binary", "git-binary"],
            &["jackin gateway add"],
        )?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn doctor_under_a_cancel_exits_cancelled_and_prints_no_report() -> TestResult {
    guard(async {
        let Fixture {
            harness,
            world: _world,
        } = healthy()?;
        let token = CancellationToken::new();
        token.cancel();
        let ran = harness.command(&["doctor"]).cancel(token).run().await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stdout.is_empty(), "{}", ran.stdout);
        for id in IDS {
            assert!(!ran.stderr.contains(id), "{id}: {}", ran.stderr);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn doctor_cancelled_while_the_gateway_is_being_discovered_exits_cancelled_at_once()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (harness, _) = connecting(&world, ProcessReply::until_cancelled(Vec::new()))?;
        let token = CancellationToken::new();
        token.cancel();
        let ran = harness.command(&["doctor"]).cancel(token).run().await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stdout.is_empty(), "{}", ran.stdout);
        for id in IDS {
            assert!(!ran.stderr.contains(id), "{id}: {}", ran.stderr);
        }
        Ok(())
    })
    .await
}
