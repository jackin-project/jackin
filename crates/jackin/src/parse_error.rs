// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsString;

use clap::CommandFactory;
use clap::error::{ContextKind, ContextValue, Error, ErrorKind};

use crate::cli::Cli;

fn usage_of(args: &[OsString]) -> String {
    let mut root = Cli::command();
    root.build();
    let mut command = &root;
    for arg in args.iter().skip(1) {
        if let Some(found) = arg.to_str().and_then(|name| command.find_subcommand(name)) {
            command = found;
        }
    }
    command.clone().render_usage().to_string()
}

pub(crate) fn render(error: &Error, args: &[OsString]) -> String {
    let (what, names_argument) = match error.kind() {
        ErrorKind::InvalidValue | ErrorKind::ValueValidation => ("invalid value", true),
        ErrorKind::UnknownArgument => ("unexpected argument", false),
        ErrorKind::InvalidSubcommand => ("unrecognized subcommand", false),
        ErrorKind::NoEquals => ("a value needs an equals sign", true),
        ErrorKind::TooManyValues => ("too many values", true),
        ErrorKind::TooFewValues => ("too few values", true),
        ErrorKind::WrongNumberOfValues => ("wrong number of values", true),
        ErrorKind::ArgumentConflict => ("conflicting arguments", true),
        ErrorKind::MissingRequiredArgument => ("missing required arguments", true),
        ErrorKind::MissingSubcommand => ("a subcommand is required", false),
        ErrorKind::InvalidUtf8 => ("an argument is not valid UTF-8", false),
        _ => ("the command line is invalid", false),
    };
    let mut text = format!("error: {what}");
    if names_argument && let Some(argument) = text_of(error, ContextKind::InvalidArg) {
        text.push_str(&format!(" for '{argument}'"));
    }
    if let Some(ContextValue::Strings(valid)) = error.get(ContextKind::ValidValue) {
        text.push_str(&format!("\nexpected one of: {}", valid.join(", ")));
    }
    let usage = text_of(error, ContextKind::Usage).unwrap_or_else(|| usage_of(args));
    text.push_str(&format!(
        "\n\n{usage}\n\nFor more information, try '--help'."
    ));
    text
}

fn text_of(error: &Error, kind: ContextKind) -> Option<String> {
    match error.get(kind)? {
        ContextValue::String(text) => Some(text.clone()),
        ContextValue::Strings(list) => Some(list.join(", ")),
        ContextValue::StyledStr(text) => Some(text.to_string()),
        _ => None,
    }
}
