// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_testkit::{ProcessReply, ScriptedProgram, TestResult, World, guard};
use tokio_util::sync::CancellationToken;

use super::support::{Failure, FixedConnector, Harness, failed};

const IDS: [&str; 7] = [
    "openshell-binary",
    "ssh-binary",
    "git-binary",
    "gateway-registered",
    "gateway-health",
    "pinned-version",
    "engine-socket",
];

const CONFIG: &str = "default_gateway = \"fake-gateway\"

[gateways.fake-gateway]
openshell_workspace = \"default\"
builder = \"b\"
platform = \"linux/arm64\"
";

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
    harness.write_config(CONFIG)?;
    Ok(Fixture { world, harness })
}

fn healthy() -> Result<Fixture, Failure> {
    fixture(true, ProcessReply::exited(0, b"openshell 0.1.6\n", b""))
}

#[tokio::test]
async fn healthy_fakes_exit_ok_and_print_one_line_per_check_id_on_stderr_only() -> TestResult {
    guard(async {
        let Fixture {
            harness,
            world: _world,
        } = healthy()?;
        let ran = harness.run(&["doctor"]).await?;
        ran.ok()?;
        assert!(ran.stdout.is_empty());
        for id in IDS {
            assert_eq!(ran.stderr.matches(id).count(), 1, "{id}: {}", ran.stderr);
        }
        Ok(())
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
        assert!(ran.stderr.contains("warn"), "{}", ran.stderr);
        assert!(ran.stderr.contains("engine-socket"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failed_check_fails_the_run_names_its_id_and_never_hides_the_others() -> TestResult {
    guard(async {
        let Fixture { world, harness } = healthy()?;
        world.gateway.set_serving(false);
        let ran = harness.run(&["doctor"]).await?;
        failed(&ran, ExitClass::Failure, &IDS)?;

        let Fixture {
            mut harness,
            world: _kept,
        } = healthy()?;
        harness.connector = FixedConnector::empty();
        let ran = harness.run(&["doctor"]).await?;
        failed(&ran, ExitClass::Failure, &IDS)?;
        assert!(ran.stderr.contains("fail"));

        let Fixture {
            harness,
            world: _world,
        } = fixture(true, ProcessReply::spawn("not found"))?;
        let ran = harness.run(&["doctor"]).await?;
        failed(
            &ran,
            ExitClass::Failure,
            &["openshell-binary", "ssh-binary", "engine-socket"],
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
        failed(
            &ran,
            ExitClass::Failure,
            &["jackin gateway add", "openshell-binary", "git-binary"],
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
        assert!(ran.stdout.is_empty());
        for id in IDS {
            assert!(!ran.stderr.contains(id), "{id}: {}", ran.stderr);
        }
        Ok(())
    })
    .await
}
