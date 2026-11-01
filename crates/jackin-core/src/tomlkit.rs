// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use toml::{Table, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Finding {
    Unknown { key: String },
    Missing { key: String },
    Invalid { key: String, message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyntaxProblem {
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) message: String,
}

pub(crate) fn parse_table(text: &str) -> Result<Table, SyntaxProblem> {
    text.parse::<Table>().map_err(|error| {
        let offset = error.span().map_or(0, |span| span.start).min(text.len());
        let (line, column) = position(text.as_bytes(), offset);
        SyntaxProblem {
            line,
            column,
            message: error.message().to_owned(),
        }
    })
}

pub(crate) fn position(bytes: &[u8], offset: usize) -> (usize, usize) {
    let before = &bytes[..offset.min(bytes.len())];
    let line = before.split(|byte| *byte == b'\n').count();
    let line_start = before
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    (line, before.len() - line_start + 1)
}

pub(crate) fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

#[must_use]
#[derive(Debug)]
pub(crate) struct Reader<'t, 's> {
    path: String,
    table: &'t Table,
    used: BTreeSet<&'static str>,
    sink: &'s mut Vec<Finding>,
}

impl<'t, 's> Reader<'t, 's> {
    pub(crate) fn new(path: String, table: &'t Table, sink: &'s mut Vec<Finding>) -> Self {
        Reader {
            path,
            table,
            used: BTreeSet::new(),
            sink,
        }
    }

    pub(crate) fn path_of(&self, key: &str) -> String {
        join(&self.path, key)
    }

    pub(crate) fn report_invalid(&mut self, key: &str, message: &str) {
        let key = self.path_of(key);
        self.sink.push(Finding::Invalid {
            key,
            message: message.to_owned(),
        });
    }

    pub(crate) fn report_invalid_here(&mut self, message: &str) {
        self.sink.push(Finding::Invalid {
            key: self.path.clone(),
            message: message.to_owned(),
        });
    }

    pub(crate) fn contains(&self, key: &str) -> bool {
        self.table.contains_key(key)
    }

    pub(crate) fn report_missing(&mut self, key: &str) {
        let key = self.path_of(key);
        self.sink.push(Finding::Missing { key });
    }

    pub(crate) fn report_unknown(&mut self, key: &str) {
        let key = self.path_of(key);
        self.sink.push(Finding::Unknown { key });
    }

    pub(crate) fn value(&mut self, key: &'static str) -> Option<&'t Value> {
        self.used.insert(key);
        self.table.get(key)
    }

    pub(crate) fn string(&mut self, key: &'static str) -> Option<&'t str> {
        let Value::String(text) = self.value(key)? else {
            self.report_invalid(key, "expected a string");
            return None;
        };
        Some(text.as_str())
    }

    pub(crate) fn required_string(&mut self, key: &'static str) -> Option<&'t str> {
        if !self.table.contains_key(key) {
            self.used.insert(key);
            self.report_missing(key);
            return None;
        }
        self.string(key)
    }

    pub(crate) fn boolean(&mut self, key: &'static str) -> Option<bool> {
        let Value::Boolean(flag) = self.value(key)? else {
            self.report_invalid(key, "expected true or false");
            return None;
        };
        Some(*flag)
    }

    pub(crate) fn integer(&mut self, key: &'static str) -> Option<i64> {
        let Value::Integer(number) = self.value(key)? else {
            self.report_invalid(key, "expected an integer");
            return None;
        };
        Some(*number)
    }

    pub(crate) fn required_integer(&mut self, key: &'static str) -> Option<i64> {
        if !self.table.contains_key(key) {
            self.used.insert(key);
            self.report_missing(key);
            return None;
        }
        self.integer(key)
    }

    pub(crate) fn parsed<T, E>(
        &mut self,
        key: &'static str,
        parse: impl FnOnce(&str) -> Result<T, E>,
        describe: impl FnOnce(E) -> String,
    ) -> Option<T> {
        let text = self.string(key)?;
        match parse(text) {
            Ok(value) => Some(value),
            Err(error) => {
                self.report_invalid(key, &describe(error));
                None
            }
        }
    }

    pub(crate) fn required_parsed<T, E>(
        &mut self,
        key: &'static str,
        parse: impl FnOnce(&str) -> Result<T, E>,
        describe: impl FnOnce(E) -> String,
    ) -> Option<T> {
        if !self.table.contains_key(key) {
            self.used.insert(key);
            self.report_missing(key);
            return None;
        }
        self.parsed(key, parse, describe)
    }

    pub(crate) fn table(&mut self, key: &'static str) -> Option<Reader<'t, '_>> {
        let Value::Table(table) = self.value(key)? else {
            self.report_invalid(key, "expected a table");
            return None;
        };
        let path = self.path_of(key);
        Some(Reader::new(path, table, &mut *self.sink))
    }

    pub(crate) fn required_table(&mut self, key: &'static str) -> Option<Reader<'t, '_>> {
        if !self.table.contains_key(key) {
            self.used.insert(key);
            self.report_missing(key);
            return None;
        }
        self.table(key)
    }

    pub(crate) fn entries(&self) -> Vec<(&'t str, &'t Value)> {
        self.table
            .iter()
            .map(|(key, value)| (key.as_str(), value))
            .collect()
    }

    pub(crate) fn entry_table(&mut self, key: &str, value: &'t Value) -> Option<Reader<'t, '_>> {
        let Value::Table(table) = value else {
            self.report_invalid(key, "expected a table");
            return None;
        };
        let path = self.path_of(key);
        Some(Reader::new(path, table, &mut *self.sink))
    }

    pub(crate) fn entry_parsed<T, E>(
        &mut self,
        key: &str,
        value: &Value,
        parse: impl FnOnce(&str) -> Result<T, E>,
        describe: impl FnOnce(E) -> String,
    ) -> Option<T> {
        let Value::String(text) = value else {
            self.report_invalid(key, "expected a string");
            return None;
        };
        match parse(text) {
            Ok(parsed) => Some(parsed),
            Err(error) => {
                self.report_invalid(key, &describe(error));
                None
            }
        }
    }

    pub(crate) fn finish(self) {
        for key in self.table.keys() {
            if !self.used.contains(key.as_str()) {
                let key = join(&self.path, key);
                self.sink.push(Finding::Unknown { key });
            }
        }
    }
}

pub(crate) fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ch if ch.is_control() => {
                out.push_str(&format!("\\u{:04X}", u32::from(ch)));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

pub(crate) fn key(name: &str) -> String {
    let bare = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    if bare { name.to_owned() } else { quote(name) }
}
