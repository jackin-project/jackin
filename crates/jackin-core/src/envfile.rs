// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use thiserror::Error;

use crate::ids::EnvName;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvFile(BTreeMap<EnvName, String>);

#[derive(Debug, Error)]
pub enum EnvFileError {
    #[error("invalid value for {name}")]
    InvalidValue { name: EnvName },
    #[error("syntax error at line {line}")]
    Syntax { line: usize },
}

fn parse_line(line: &str) -> Option<(EnvName, String)> {
    let (name, quoted) = line.strip_prefix("export ")?.split_once('=')?;
    let name = EnvName::parse(name).ok()?;
    let mut rest = quoted.strip_prefix('\'')?;
    let mut value = String::new();
    loop {
        let (chunk, after) = rest.split_once('\'')?;
        value.push_str(chunk);
        match after.strip_prefix("\\''") {
            Some(tail) => {
                value.push('\'');
                rest = tail;
            }
            None => {
                return (after.is_empty() && !value.contains('\0')).then_some((name, value));
            }
        }
    }
}

impl EnvFile {
    pub fn new(values: BTreeMap<EnvName, String>) -> Result<EnvFile, EnvFileError> {
        match values
            .iter()
            .find(|(_, value)| value.contains(['\n', '\0']))
        {
            Some((name, _)) => Err(EnvFileError::InvalidValue { name: name.clone() }),
            None => Ok(EnvFile(values)),
        }
    }

    pub fn parse(text: &str) -> Result<EnvFile, EnvFileError> {
        let mut values = BTreeMap::new();
        let mut remaining = text;
        let mut line_number = 0;
        while !remaining.is_empty() {
            line_number += 1;
            let syntax = EnvFileError::Syntax { line: line_number };
            let Some((line, rest)) = remaining.split_once('\n') else {
                return Err(syntax);
            };
            remaining = rest;
            let Some((name, value)) = parse_line(line) else {
                return Err(syntax);
            };
            if values
                .last_key_value()
                .is_some_and(|(last, _)| *last >= name)
            {
                return Err(syntax);
            }
            values.insert(name, value);
        }
        Ok(EnvFile(values))
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for (name, value) in &self.0 {
            out.push_str("export ");
            out.push_str(name.as_str());
            out.push_str("='");
            out.push_str(&value.replace('\'', "'\\''"));
            out.push_str("'\n");
        }
        out
    }

    pub fn values(&self) -> &BTreeMap<EnvName, String> {
        &self.0
    }
}
