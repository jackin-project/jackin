// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::CommandFactory;
use jackin::{Cli, ExitClass};
use jackin_testkit::{TestResult, World, guard};

use super::support::{Failure, Harness, ScriptedTerminal};

const COMMANDS: &[&str] = &[
    "gateway",
    "gateway add",
    "gateway ls",
    "gateway rm",
    "build",
    "load",
    "attach",
    "shell",
    "export",
    "stop",
    "start",
    "purge",
    "doctor",
    "ls",
    "usage",
    "inspect",
    "account",
    "account add",
    "account ls",
    "account rm",
    "account default",
    "account bind",
    "role",
    "role add",
    "role ls",
    "role rm",
    "role check",
    "workspace",
    "workspace add",
    "workspace ls",
    "workspace rm",
];

fn walk(command: &clap::Command, path: &str, seen: &mut Vec<String>) -> Result<(), Failure> {
    for arg in command.get_arguments() {
        let name = arg.get_id().as_str();
        if matches!(name, "help" | "version") {
            continue;
        }
        let help = arg.get_help().map(ToString::to_string).unwrap_or_default();
        if help.trim().is_empty() {
            return Err(format!("argument {name} of {path:?} has no help").into());
        }
    }
    for sub in command.get_subcommands() {
        if sub.get_name() == "help" {
            continue;
        }
        let full = if path.is_empty() {
            sub.get_name().to_owned()
        } else {
            format!("{path} {}", sub.get_name())
        };
        let about = sub.get_about().map(ToString::to_string).unwrap_or_default();
        if about.trim().is_empty() {
            return Err(format!("command {full:?} has no about").into());
        }
        seen.push(full.clone());
        walk(sub, &full, seen)?;
    }
    Ok(())
}

#[test]
fn every_command_and_argument_has_attribute_help_and_the_command_set_is_the_documented_one()
-> Result<(), Failure> {
    let command = Cli::command();
    let about = command
        .get_about()
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(!about.trim().is_empty());
    let mut seen = Vec::new();
    walk(&command, "", &mut seen)?;
    seen.sort();
    let mut wanted: Vec<String> = COMMANDS.iter().map(|name| (*name).to_owned()).collect();
    wanted.sort();
    assert_eq!(seen, wanted);
    Ok(())
}

#[test]
fn the_clap_definition_is_consistent() {
    Cli::command().debug_assert();
}

#[tokio::test]
async fn a_bare_invocation_prints_help_on_a_terminal_too() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        let (terminal, _) = ScriptedTerminal::new(Vec::new())?;
        let on_terminal = harness.command(&[]).terminal(terminal).run().await?;
        let plain = harness.run(&["--help"]).await?;
        assert_eq!(on_terminal.exit, ExitClass::Ok);
        assert_eq!(on_terminal.stdout, plain.stdout);
        assert!(on_terminal.stderr.is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn help_and_version_print_to_stdout_and_a_bare_invocation_prints_help_without_a_gateway()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let mut harness = Harness::new(&world);
        harness.connector = super::support::FixedConnector::empty();
        let help = harness.run(&["--help"]).await?;
        assert_eq!(help.exit, ExitClass::Ok);
        assert!(help.stdout.contains("Usage"), "{}", help.stdout);
        assert!(help.stderr.is_empty(), "{}", help.stderr);

        let version = harness.run(&["--version"]).await?;
        assert_eq!(version.exit, ExitClass::Ok);
        assert!(version.stdout.contains(env!("CARGO_PKG_VERSION")));
        assert!(version.stderr.is_empty());

        let bare = harness.run(&[]).await?;
        assert_eq!(bare.exit, ExitClass::Ok);
        assert_eq!(bare.stdout, help.stdout);
        assert!(bare.stderr.is_empty());
        Ok(())
    })
    .await
}
