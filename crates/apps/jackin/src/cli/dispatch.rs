// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::IsTerminal;

use crate::cli::role::ConsoleArgs;
use crate::cli::{Cli, Command};

pub const MIN_TUI_COLS: u16 = 40;

pub const MIN_TUI_ROWS: u16 = 15;

pub const CONSOLE_REQUIRES_TTY_ERROR: &str =
    "jackin❯ console requires an interactive terminal (stdout must be a TTY, minimum size 40x15)";

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    RunConsole { args: ConsoleArgs, explicit: bool },

    RunCommand(Box<Command>),
    PrintHelpAndExit,
    PrintHelp { command: Vec<String> },
    ErrorNotTtyCapable,
}

pub fn is_tui_capable() -> bool {
    if !std::io::stdout().is_terminal() {
        return false;
    }
    match crossterm::terminal::size() {
        Ok((cols, rows)) => is_size_tui_capable(cols, rows),
        Err(_) => false,
    }
}

#[must_use]
pub const fn is_size_tui_capable(cols: u16, rows: u16) -> bool {
    cols >= MIN_TUI_COLS && rows >= MIN_TUI_ROWS
}

pub fn classify(cli: Cli, tui_capable: bool) -> Action {
    match cli.command {
        Some(Command::Console(args)) => {
            if tui_capable {
                Action::RunConsole {
                    args,
                    explicit: true,
                }
            } else {
                Action::ErrorNotTtyCapable
            }
        }
        Some(Command::Help { command }) => Action::PrintHelp { command },
        Some(other) => Action::RunCommand(Box::new(other)),
        None => {
            if tui_capable {
                Action::RunConsole {
                    args: cli.console_args,
                    explicit: false,
                }
            } else {
                Action::PrintHelpAndExit
            }
        }
    }
}

#[cfg(test)]
mod tests;
