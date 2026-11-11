// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{self, Debug};
use std::io;

use async_trait::async_trait;
use jackin_openshell::TermSize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalInput {
    Bytes(Vec<u8>),
    Resized(TermSize),
    Closed,
}

pub struct RawMode {
    _restore: Box<dyn Send>,
}

impl RawMode {
    pub fn new(restore: impl Send + 'static) -> RawMode {
        RawMode {
            _restore: Box::new(restore),
        }
    }
}

impl Debug for RawMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawMode").finish_non_exhaustive()
    }
}

#[async_trait]
pub trait Terminal: Send + Debug {
    fn size(&self) -> TermSize;
    fn enter_raw(&mut self) -> io::Result<RawMode>;
    async fn read_line(&mut self) -> io::Result<Option<String>>;
    async fn next_input(&mut self) -> TerminalInput;
}
