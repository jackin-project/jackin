// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{self, Debug};
use std::io::Write;

use crate::error::CommandError;

pub(crate) struct Io {
    stdout: Box<dyn Write + Send>,
    stderr: Box<dyn Write + Send>,
}

impl Debug for Io {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Io").finish_non_exhaustive()
    }
}

impl Io {
    pub(crate) fn new(stdout: Box<dyn Write + Send>, stderr: Box<dyn Write + Send>) -> Io {
        Io { stdout, stderr }
    }

    pub(crate) fn data(&mut self, text: &str) -> Result<(), CommandError> {
        self.stdout
            .write_all(text.as_bytes())
            .and_then(|()| self.stdout.flush())
            .map_err(CommandError::Output)
    }

    pub(crate) fn note(&mut self, text: &str) {
        drop(self.stderr.write_all(format!("{text}\n").as_bytes()));
        drop(self.stderr.flush());
    }
}
