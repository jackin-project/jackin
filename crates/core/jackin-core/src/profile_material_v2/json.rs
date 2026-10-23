// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use serde_json::value::RawValue;

use super::error::ProfileCredentialV2Error;

pub(super) const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_JSON_VALUES: usize = 65_536;
pub(super) const MAX_CONTAINER_DEPTH: usize = 127;

pub(super) enum JsonNode<'a> {
    Null,
    Bool(bool),
    Number(&'a [u8]),
    String(String),
    Array(Vec<JsonNode<'a>>),
    Object(Vec<(String, JsonNode<'a>)>),
}

pub(super) fn parse_json(raw: &[u8]) -> Result<JsonNode<'_>, ProfileCredentialV2Error> {
    if raw.len() > MAX_INPUT_BYTES {
        return Err(ProfileCredentialV2Error::InputTooLarge);
    }

    let mut parser = JsonParser {
        raw,
        position: 0,
        values: 0,
    };
    parser.skip_whitespace()?;
    let node = parser.parse_value(0)?;
    parser.skip_whitespace()?;
    if parser.position != raw.len() {
        return Err(ProfileCredentialV2Error::InvalidJson);
    }
    Ok(node)
}

struct JsonParser<'a> {
    raw: &'a [u8],
    position: usize,
    values: usize,
}

impl<'a> JsonParser<'a> {
    fn parse_value(
        &mut self,
        open_containers: usize,
    ) -> Result<JsonNode<'a>, ProfileCredentialV2Error> {
        self.skip_whitespace()?;
        self.count_value()?;

        match self.peek() {
            Some(b'n') => {
                self.parse_literal(b"null")?;
                Ok(JsonNode::Null)
            }
            Some(b't') => {
                self.parse_literal(b"true")?;
                Ok(JsonNode::Bool(true))
            }
            Some(b'f') => {
                self.parse_literal(b"false")?;
                Ok(JsonNode::Bool(false))
            }
            Some(b'"') => self.parse_string().map(JsonNode::String),
            Some(b'[') => {
                let nested_depth = Self::enter_container(open_containers)?;
                self.parse_array(nested_depth)
            }
            Some(b'{') => {
                let nested_depth = Self::enter_container(open_containers)?;
                self.parse_object(nested_depth)
            }
            Some(b'-' | b'0'..=b'9') => self.parse_number().map(JsonNode::Number),
            _ => Err(ProfileCredentialV2Error::InvalidJson),
        }
    }

    fn parse_array(
        &mut self,
        open_containers: usize,
    ) -> Result<JsonNode<'a>, ProfileCredentialV2Error> {
        self.consume(b'[')?;
        self.skip_whitespace()?;
        let mut values = Vec::new();
        if self.peek() == Some(b']') {
            self.advance()?;
            return Ok(JsonNode::Array(values));
        }

        loop {
            values.push(self.parse_value(open_containers)?);
            self.skip_whitespace()?;
            match self.peek() {
                Some(b',') => {
                    self.advance()?;
                    self.skip_whitespace()?;
                }
                Some(b']') => {
                    self.advance()?;
                    return Ok(JsonNode::Array(values));
                }
                _ => return Err(ProfileCredentialV2Error::InvalidJson),
            }
        }
    }

    fn parse_object(
        &mut self,
        open_containers: usize,
    ) -> Result<JsonNode<'a>, ProfileCredentialV2Error> {
        self.consume(b'{')?;
        self.skip_whitespace()?;
        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        if self.peek() == Some(b'}') {
            self.advance()?;
            return Ok(JsonNode::Object(entries));
        }

        loop {
            if self.peek() == Some(b'}') {
                return Err(ProfileCredentialV2Error::InvalidJson);
            }
            self.ensure_value_slot()?;
            let key = self.parse_string()?;
            if !seen.insert(key.clone()) {
                return Err(ProfileCredentialV2Error::DuplicateKey);
            }
            self.skip_whitespace()?;
            self.consume(b':')?;
            let value = self.parse_value(open_containers)?;
            entries.push((key, value));
            self.skip_whitespace()?;
            match self.peek() {
                Some(b',') => {
                    self.advance()?;
                    self.skip_whitespace()?;
                }
                Some(b'}') => {
                    self.advance()?;
                    entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
                    return Ok(JsonNode::Object(entries));
                }
                _ => return Err(ProfileCredentialV2Error::InvalidJson),
            }
        }
    }

    fn parse_string(&mut self) -> Result<String, ProfileCredentialV2Error> {
        let start = self.position;
        self.consume(b'"')?;
        loop {
            match self.peek() {
                Some(b'"') => {
                    self.advance()?;
                    break;
                }
                Some(b'\\') => {
                    self.advance()?;
                    if self.peek().is_none() {
                        return Err(ProfileCredentialV2Error::InvalidJson);
                    }
                    self.advance()?;
                }
                Some(_) => self.advance()?,
                None => return Err(ProfileCredentialV2Error::InvalidJson),
            }
        }

        let token = self
            .raw
            .get(start..self.position)
            .ok_or(ProfileCredentialV2Error::InvalidJson)?;
        serde_json::from_slice::<String>(token).map_err(|_| ProfileCredentialV2Error::InvalidJson)
    }

    fn parse_number(&mut self) -> Result<&'a [u8], ProfileCredentialV2Error> {
        let start = self.position;
        loop {
            match self.peek() {
                None | Some(b' ' | b'\n' | b'\r' | b'\t' | b',' | b']' | b'}') => break,
                Some(_) => self.advance()?,
            }
        }

        let token = self
            .raw
            .get(start..self.position)
            .ok_or(ProfileCredentialV2Error::InvalidJson)?;
        if !matches!(token.first(), Some(b'-' | b'0'..=b'9')) {
            return Err(ProfileCredentialV2Error::InvalidJson);
        }
        let _: &RawValue =
            serde_json::from_slice(token).map_err(|_| ProfileCredentialV2Error::InvalidJson)?;
        Ok(token)
    }

    fn parse_literal(&mut self, literal: &[u8]) -> Result<(), ProfileCredentialV2Error> {
        for expected in literal {
            if self.peek() != Some(*expected) {
                return Err(ProfileCredentialV2Error::InvalidJson);
            }
            self.advance()?;
        }
        Ok(())
    }

    fn count_value(&mut self) -> Result<(), ProfileCredentialV2Error> {
        self.ensure_value_slot()?;
        let next = self
            .values
            .checked_add(1)
            .ok_or(ProfileCredentialV2Error::TooManyValues)?;
        self.values = next;
        Ok(())
    }

    fn ensure_value_slot(&self) -> Result<(), ProfileCredentialV2Error> {
        if self.values >= MAX_JSON_VALUES {
            return Err(ProfileCredentialV2Error::TooManyValues);
        }
        Ok(())
    }

    fn enter_container(open_containers: usize) -> Result<usize, ProfileCredentialV2Error> {
        if open_containers >= MAX_CONTAINER_DEPTH {
            return Err(ProfileCredentialV2Error::TooDeep);
        }
        open_containers
            .checked_add(1)
            .ok_or(ProfileCredentialV2Error::TooDeep)
    }

    fn skip_whitespace(&mut self) -> Result<(), ProfileCredentialV2Error> {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.advance()?;
        }
        Ok(())
    }

    fn consume(&mut self, expected: u8) -> Result<(), ProfileCredentialV2Error> {
        if self.peek() != Some(expected) {
            return Err(ProfileCredentialV2Error::InvalidJson);
        }
        self.advance()
    }

    fn peek(&self) -> Option<u8> {
        self.raw.get(self.position).copied()
    }

    fn advance(&mut self) -> Result<(), ProfileCredentialV2Error> {
        let next = self
            .position
            .checked_add(1)
            .ok_or(ProfileCredentialV2Error::InvalidJson)?;
        if next > self.raw.len() {
            return Err(ProfileCredentialV2Error::InvalidJson);
        }
        self.position = next;
        Ok(())
    }
}
