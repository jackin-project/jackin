// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use rustix::process::Pid;

const PGRP: usize = 2;
const THREADS: usize = 17;

struct Stat {
    state: u8,
    pgrp: i32,
    threads: u32,
}

impl Stat {
    fn is_over(&self) -> bool {
        match self.state {
            b'X' => true,
            b'Z' => self.threads == 1,
            _ => false,
        }
    }
}

fn number<T: FromStr>(field: Option<&&[u8]>) -> Option<T> {
    std::str::from_utf8(field?).ok()?.parse().ok()
}

fn parse(stat: &[u8]) -> Option<Stat> {
    let end = stat.iter().rposition(|byte| *byte == b')')?;
    let fields: Vec<&[u8]> = stat[end + 1..]
        .split(u8::is_ascii_whitespace)
        .filter(|field| !field.is_empty())
        .collect();
    Some(Stat {
        state: *fields.first()?.first()?,
        pgrp: number(fields.get(PGRP))?,
        threads: number(fields.get(THREADS))?,
    })
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Unparsable;

pub(super) fn is_live_member(stat: &[u8], group: Pid) -> Result<bool, Unparsable> {
    let stat = parse(stat).ok_or(Unparsable)?;
    Ok(stat.pgrp == group.as_raw_pid() && !stat.is_over())
}
