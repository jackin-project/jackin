// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{self, Debug};
use std::io::Write;

use jackin_core::{EnvName, EnvSnapshot};
use jackin_usage::view::{Line, render};
use serde::Serialize;

use crate::error::CommandError;

pub(crate) struct Io {
    stdout: Box<dyn Write + Send>,
    stderr: Box<dyn Write + Send>,
    color: bool,
}

pub(crate) fn wants_color(env: &EnvSnapshot, terminal: bool) -> bool {
    let value = |name: &str| {
        EnvName::parse(name)
            .ok()
            .and_then(|name| env.get(&name))
            .filter(|value| !value.is_empty())
    };
    terminal && value("NO_COLOR").is_none() && value("TERM") != Some("dumb")
}

impl Debug for Io {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Io")
            .field("color", &self.color)
            .finish_non_exhaustive()
    }
}

impl Io {
    pub(crate) fn new(
        stdout: Box<dyn Write + Send>,
        stderr: Box<dyn Write + Send>,
        color: bool,
    ) -> Io {
        Io {
            stdout,
            stderr,
            color,
        }
    }

    pub(crate) fn lines(&mut self, lines: &[Line]) -> Result<(), CommandError> {
        let text = if self.color {
            render::ansi(lines)
        } else {
            render::plain(lines)
        };
        self.data(&text)
    }

    pub(crate) fn data(&mut self, text: &str) -> Result<(), CommandError> {
        self.stdout
            .write_all(text.as_bytes())
            .and_then(|()| self.stdout.flush())
            .map_err(CommandError::Output)
    }

    pub(crate) fn line(&mut self, text: &str) -> Result<(), CommandError> {
        self.data(&format!("{text}\n"))
    }

    pub(crate) fn json<T: Serialize>(&mut self, value: &T) -> Result<(), CommandError> {
        let mut text = serde_json::to_string(value).map_err(CommandError::Json)?;
        text.push('\n');
        self.data(&text)
    }

    pub(crate) fn ask(&mut self, text: &str) {
        drop(self.stderr.write_all(text.as_bytes()));
        drop(self.stderr.flush());
    }

    pub(crate) fn bytes(&mut self, bytes: &[u8]) -> Result<(), CommandError> {
        self.stdout
            .write_all(bytes)
            .and_then(|()| self.stdout.flush())
            .map_err(CommandError::Output)
    }

    pub(crate) fn raw_note(&mut self, text: &str) {
        drop(self.stderr.write_all(format!("\r\n{text}\r\n").as_bytes()));
        drop(self.stderr.flush());
    }

    pub(crate) fn note(&mut self, text: &str) {
        drop(self.stderr.write_all(format!("{text}\n").as_bytes()));
        drop(self.stderr.flush());
    }
}

#[cfg(test)]
mod tests {
    use jackin_core::EnvSnapshot;

    use super::wants_color;

    fn env(pairs: &[(&str, &str)]) -> EnvSnapshot {
        EnvSnapshot::from_pairs(
            pairs
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned())),
        )
    }

    #[test]
    fn color_needs_a_terminal_that_was_not_opted_out() {
        let xterm = env(&[("TERM", "xterm-256color")]);
        assert!(wants_color(&xterm, true));
        assert!(wants_color(&env(&[]), true));
        assert!(wants_color(&env(&[("NO_COLOR", "")]), true));
        assert!(!wants_color(&xterm, false));
        assert!(!wants_color(&env(&[("NO_COLOR", "1")]), true));
        assert!(!wants_color(&env(&[("TERM", "dumb")]), true));
    }
}
