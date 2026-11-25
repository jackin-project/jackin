// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Index;

use serde_json::value::RawValue;

use crate::decimal::{Decimal, DecimalError};
use crate::error::ProviderError;

const NOT_JSON: ProviderError = ProviderError::Decode("the response is not valid JSON");
const MAX_DEPTH: usize = 16;

static ABSENT: Exact = Exact::Null;

#[derive(Clone, PartialEq)]
pub enum Exact {
    Null,
    Bool(bool),
    Number(String),
    Text(String),
    List(Vec<Exact>),
    Map(BTreeMap<String, Exact>),
}

impl Exact {
    pub fn parse(bytes: &[u8]) -> Result<Exact, ProviderError> {
        if !shallow(bytes) {
            return Err(NOT_JSON);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| NOT_JSON)?;
        let raw: Box<RawValue> = serde_json::from_str(text).map_err(|_| NOT_JSON)?;
        build(&raw)
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Exact::Bool(flag) => Some(*flag),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Exact::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Exact]> {
        match self {
            Exact::List(items) => Some(items),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Exact::Null)
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Exact::Map(_))
    }

    pub fn decimal(&self) -> Result<Option<Decimal>, ProviderError> {
        let parsed = match self {
            Exact::Number(lexeme) => Decimal::parse_number(lexeme),
            Exact::Text(text) => Decimal::parse_text(text),
            _ => return Ok(None),
        };
        match parsed {
            Ok(amount) => Ok(Some(amount)),
            Err(DecimalError::Invalid) => Ok(None),
            Err(DecimalError::OutOfRange) => Err(ProviderError::OUT_OF_RANGE),
        }
    }
}

impl fmt::Debug for Exact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Exact(<redacted>)")
    }
}

impl Index<&str> for Exact {
    type Output = Exact;

    fn index(&self, key: &str) -> &Exact {
        match self {
            Exact::Map(members) => members.get(key).unwrap_or(&ABSENT),
            _ => &ABSENT,
        }
    }
}

fn shallow(bytes: &[u8]) -> bool {
    let (mut depth, mut quoted, mut escaped) = (0_usize, false, false);
    for byte in bytes {
        if quoted {
            match (escaped, byte) {
                (true, _) => escaped = false,
                (false, b'\\') => escaped = true,
                (false, b'"') => quoted = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => quoted = true,
            b'{' | b'[' => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return false;
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    true
}

fn build(raw: &RawValue) -> Result<Exact, ProviderError> {
    let text = raw.get();
    match text.as_bytes().first() {
        Some(b'{') => {
            let members: BTreeMap<String, Box<RawValue>> =
                serde_json::from_str(text).map_err(|_| NOT_JSON)?;
            members
                .into_iter()
                .map(|(name, member)| Ok((name, build(&member)?)))
                .collect::<Result<_, ProviderError>>()
                .map(Exact::Map)
        }
        Some(b'[') => {
            let items: Vec<Box<RawValue>> = serde_json::from_str(text).map_err(|_| NOT_JSON)?;
            items
                .iter()
                .map(|item| build(item))
                .collect::<Result<_, _>>()
                .map(Exact::List)
        }
        Some(b'"') => serde_json::from_str(text)
            .map(Exact::Text)
            .map_err(|_| NOT_JSON),
        Some(b't') => Ok(Exact::Bool(true)),
        Some(b'f') => Ok(Exact::Bool(false)),
        Some(b'n') => Ok(Exact::Null),
        Some(_) => Ok(Exact::Number(text.to_owned())),
        None => Err(NOT_JSON),
    }
}
