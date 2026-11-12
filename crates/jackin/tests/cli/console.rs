// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use jackin::ExitClass;
use jackin_core::{GatewayName, HostPath};
use jackin_testkit::{TestResult, World, guard};
use jackin_tui::{ConsoleDeps, ConsoleError, ConsoleExit, ConsoleHost, build_console};

use super::support::{Failure, GATEWAY_CONFIG, Harness, ScriptedTerminal};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Run {
    pub(crate) default: Option<GatewayName>,
    pub(crate) reached: Vec<String>,
    pub(crate) workspaces: usize,
    pub(crate) cwd: HostPath,
    pub(crate) home: HostPath,
    pub(crate) connect_deadline: Duration,
    pub(crate) supervisor_poll: Duration,
}

#[derive(Debug, Default)]
pub(crate) struct ScriptedConsole {
    calls: AtomicUsize,
    runs: Mutex<Vec<Run>>,
    fail: Mutex<Option<String>>,
    cancel: Mutex<bool>,
    names: Mutex<Vec<String>>,
}

impl ScriptedConsole {
    pub(crate) fn runs(&self) -> Vec<Run> {
        self.runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub(crate) fn fail_with(&self, message: &str) {
        *self.fail.lock().unwrap_or_else(PoisonError::into_inner) = Some(message.to_owned());
    }

    pub(crate) fn cancel(&self) {
        *self.cancel.lock().unwrap_or_else(PoisonError::into_inner) = true;
    }

    pub(crate) fn watch(&self, gateways: &[&str]) {
        *self.names.lock().unwrap_or_else(PoisonError::into_inner) =
            gateways.iter().map(|name| (*name).to_owned()).collect();
    }
}

#[async_trait]
impl ConsoleHost for ScriptedConsole {
    async fn run(&self, deps: ConsoleDeps) -> Result<ConsoleExit, ConsoleError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let names = self
            .names
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let reached = names
            .iter()
            .filter(|name| {
                GatewayName::parse(name).is_ok_and(|name| deps.sessions.get(&name).is_some())
            })
            .cloned()
            .collect();
        let default = deps.sessions.default_name();
        let (cwd, home) = (deps.cwd.clone(), deps.home.clone());
        let (connect_deadline, supervisor_poll) =
            (deps.limits.connect_deadline, deps.limits.supervisor_poll);
        let mut console = build_console(deps).await?;
        let workspaces = console.app().world.workspaces.len();
        self.runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Run {
                default,
                reached,
                workspaces,
                cwd,
                home,
                connect_deadline,
                supervisor_poll,
            });
        if let Some(message) = self
            .fail
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            return Err(ConsoleError::Terminal(io::Error::other(message)));
        }
        if *self.cancel.lock().unwrap_or_else(PoisonError::into_inner) {
            return Err(ConsoleError::Cancelled);
        }
        Ok(ConsoleExit::Quit)
    }
}

fn two_workspaces(world: &World) -> Result<String, Failure> {
    let mut text = GATEWAY_CONFIG.to_owned();
    for name in ["alpha", "beta"] {
        let dir = world.home().as_path().join(name);
        std::fs::create_dir_all(&dir)?;
        text.push_str(&format!(
            "\n[workspaces.{name}]\npath = \"{}\"\n",
            dir.display()
        ));
    }
    Ok(text)
}

fn terminal() -> Result<ScriptedTerminal, Failure> {
    Ok(ScriptedTerminal::new(Vec::new())?.0)
}

#[tokio::test]
async fn a_bare_run_on_a_terminal_opens_the_console_over_the_config_and_quit_is_ok() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config(&two_workspaces(&world)?)?;
        harness.console.watch(&["fake-gateway"]);
        let ran = harness.command(&[]).terminal(terminal()?).run().await?;
        assert_eq!(ran.exit, ExitClass::Ok, "{}", ran.stderr);
        assert!(ran.stdout.is_empty(), "{}", ran.stdout);
        assert!(ran.stderr.is_empty(), "{}", ran.stderr);
        assert_eq!(
            harness.console.runs(),
            vec![Run {
                default: Some(GatewayName::parse("fake-gateway")?),
                reached: vec!["fake-gateway".to_owned()],
                workspaces: 2,
                cwd: harness.cwd.clone(),
                home: world.home().clone(),
                connect_deadline: Duration::from_secs(5),
                supervisor_poll: Duration::from_millis(250),
            }]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_bare_run_without_a_terminal_prints_help_and_never_starts_the_console() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config(&two_workspaces(&world)?)?;
        let help = harness.run(&["--help"]).await?;
        let bare = harness.run(&[]).await?;
        assert_eq!(bare.exit, ExitClass::Ok);
        assert_eq!(bare.stdout, help.stdout);
        assert!(bare.stdout.contains("Usage"), "{}", bare.stdout);
        assert!(bare.stderr.is_empty(), "{}", bare.stderr);
        assert_eq!(harness.console.calls(), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_terminal_failure_in_the_console_is_a_failure_on_stderr_with_empty_stdout() -> TestResult
{
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config(&two_workspaces(&world)?)?;
        harness.console.fail_with("raw mode refused");
        let ran = harness.command(&[]).terminal(terminal()?).run().await?;
        assert_eq!(ran.exit, ExitClass::Failure);
        assert!(ran.stdout.is_empty(), "{}", ran.stdout);
        assert!(
            ran.stderr.contains("terminal error: raw mode refused"),
            "{}",
            ran.stderr
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancelled_console_exits_with_the_cancelled_class() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config(&two_workspaces(&world)?)?;
        harness.console.cancel();
        let ran = harness.command(&[]).terminal(terminal()?).run().await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stdout.is_empty(), "{}", ran.stdout);
        assert!(ran.stderr.contains("cancelled"), "{}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_invalid_config_stops_before_the_console() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config("workspaces = [")?;
        let ran = harness.command(&[]).terminal(terminal()?).run().await?;
        assert_eq!(ran.exit, ExitClass::Failure);
        assert!(ran.stdout.is_empty(), "{}", ran.stdout);
        assert!(ran.stderr.starts_with("error: "), "{}", ran.stderr);
        assert_eq!(harness.console.calls(), 0);
        Ok(())
    })
    .await
}

const REFUSED_PEER: &str = "
[gateways.refused]
openshell_workspace = \"default\"
builder = \"b\"
platform = \"linux/arm64\"
";

#[tokio::test]
async fn a_gateway_that_cannot_be_reached_does_not_stop_the_console() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config(&(two_workspaces(&world)? + REFUSED_PEER))?;
        harness.console.watch(&["fake-gateway", "refused"]);
        let ran = harness.command(&[]).terminal(terminal()?).run().await?;
        assert_eq!(ran.exit, ExitClass::Ok, "{}", ran.stderr);
        let runs = harness.console.runs();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].reached, vec!["fake-gateway".to_owned()]);
        assert_eq!(runs[0].workspaces, 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_config_without_a_gateway_still_opens_the_console() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config("")?;
        harness.console.watch(&["fake-gateway"]);
        let ran = harness.command(&[]).terminal(terminal()?).run().await?;
        assert_eq!(ran.exit, ExitClass::Ok, "{}", ran.stderr);
        let runs = harness.console.runs();
        assert_eq!(runs.len(), 1);
        assert!(runs[0].reached.is_empty());
        assert_eq!(runs[0].default, None);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_gateway_flag_picks_the_default_and_an_unknown_one_stops_before_the_console()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config(&(two_workspaces(&world)? + REFUSED_PEER))?;
        let chosen = harness
            .command(&["--gateway", "refused"])
            .terminal(terminal()?)
            .run()
            .await?;
        assert_eq!(chosen.exit, ExitClass::Ok, "{}", chosen.stderr);
        assert_eq!(
            harness.console.runs()[0].default,
            Some(GatewayName::parse("refused")?)
        );
        let unknown = harness
            .command(&["--gateway", "nowhere"])
            .terminal(terminal()?)
            .run()
            .await?;
        assert_eq!(unknown.exit, ExitClass::Failure);
        assert!(unknown.stderr.contains("nowhere"), "{}", unknown.stderr);
        assert_eq!(harness.console.calls(), 1);
        Ok(())
    })
    .await
}

const TWO_WITHOUT_DEFAULT: &str = "
[gateways.first]
openshell_workspace = \"default\"
builder = \"b\"
platform = \"linux/arm64\"

[gateways.second]
openshell_workspace = \"default\"
builder = \"b\"
platform = \"linux/arm64\"
";

#[tokio::test]
async fn several_gateways_with_no_default_open_the_console_with_none_selected() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config(TWO_WITHOUT_DEFAULT)?;
        let ran = harness.command(&[]).terminal(terminal()?).run().await?;
        assert_eq!(ran.exit, ExitClass::Ok, "{}", ran.stderr);
        assert_eq!(harness.console.runs()[0].default, None);
        Ok(())
    })
    .await
}
