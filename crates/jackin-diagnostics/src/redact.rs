use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

const REDACTED: &str = "<redacted>";
const MAX_STREAM_LINE_BYTES: usize = 64 * 1024;
const MAX_STRUCTURED_DEPTH: usize = 256;
const MAX_ACTIVE_ENVELOPES: usize = 64;

/// Incremental redactor for one output stream.
///
/// The caller must use a distinct instance for stdout and stderr. Lines are
/// held until their newline arrives, and incomplete sensitive values are
/// tracked separately by verified BuildKit step envelope. Unframed records
/// cannot continue an open BuildKit value. Malformed values fail closed for
/// their envelope; an overlong or invalid-UTF-8 record closes the whole stream.
#[derive(Debug)]
pub struct StreamRedactor {
    line: Vec<u8>,
    states: HashMap<Option<String>, StreamMode>,
    failed_closed: bool,
}

impl Default for StreamRedactor {
    fn default() -> Self {
        Self {
            line: Vec::new(),
            states: HashMap::new(),
            failed_closed: false,
        }
    }
}

#[derive(Debug)]
enum StreamMode {
    Normal,
    Pem,
    Block(BlockState),
    Indented(IndentedState),
    Quoted(QuotedState),
    Structured(StructuredState),
    FailedClosed,
}

#[derive(Debug)]
struct BlockState {
    header_indent: usize,
    body_indent: Option<usize>,
}

#[derive(Debug)]
struct IndentedState {
    header_indent: usize,
    body_indent: Option<usize>,
}

#[derive(Debug)]
struct QuotedState {
    quote: u8,
    delimiter_len: usize,
    escaped: bool,
    allow_json_suffix: bool,
}

#[derive(Debug)]
struct StructuredState {
    expected_closers: Vec<u8>,
    in_string: Option<u8>,
    escaped: bool,
    malformed: bool,
}

struct BuildKitLine<'a> {
    prefix: &'a str,
    payload: &'a str,
    step: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct Assignment {
    start: usize,
    value_start: usize,
    separator: u8,
    is_authorization: bool,
}

impl StreamRedactor {
    /// Feed arbitrary byte chunks and return only complete, safe log lines.
    /// CRLF is normalized by trimming the carriage return before redaction.
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut output = Vec::new();
        for &byte in bytes {
            if self.failed_closed {
                continue;
            }
            if byte == b'\n' {
                let line = match String::from_utf8(std::mem::take(&mut self.line)) {
                    Ok(line) => line,
                    Err(_) => {
                        self.fail_closed();
                        output.push(REDACTED.to_owned());
                        continue;
                    }
                };
                let line = line.trim_end_matches('\r');
                output.extend(self.process_line(line));
            } else if self.line.len() < MAX_STREAM_LINE_BYTES {
                self.line.push(byte);
            } else {
                // Once a physical line is too large to inspect in bounded
                // memory, suppress it and the rest of this stream. It may be
                // the continuation of a sensitive value.
                self.line.clear();
                self.fail_closed();
                output.push(REDACTED.to_owned());
            }
        }
        output
    }

    /// Process complete records while retaining secret contexts between
    /// calls. This is for line-oriented sinks whose caller already knows the
    /// physical record boundary; byte-stream callers should use `push_bytes`.
    pub fn push_complete_text(&mut self, text: &str) -> String {
        if text.len() > MAX_STREAM_LINE_BYTES {
            self.fail_closed();
            return REDACTED.to_owned();
        }
        let mut output = String::new();
        let mut remaining = text;
        while let Some(newline) = remaining.find('\n') {
            let record = &remaining[..newline];
            let (record, ending) = record
                .strip_suffix('\r')
                .map_or((record, "\n"), |without_cr| (without_cr, "\r\n"));
            append_record(&mut output, self.process_line(record), ending);
            remaining = &remaining[newline + 1..];
        }
        if !remaining.is_empty() {
            append_record(&mut output, self.process_line(remaining), "");
        }
        output
    }

    /// Finish the stream. An unterminated line is redacted as a complete
    /// final line, then pending secret contexts are discarded at EOF.
    pub fn finish(&mut self) -> Vec<String> {
        let mut output = Vec::new();
        if !self.line.is_empty() && !self.failed_closed {
            let line = match String::from_utf8(std::mem::take(&mut self.line)) {
                Ok(line) => line,
                Err(_) => {
                    self.fail_closed();
                    self.reset();
                    return vec![REDACTED.to_owned()];
                }
            };
            output.extend(self.process_line(line.trim_end_matches('\r')));
        }
        self.reset();
        output
    }

    /// Drop pending bytes and secret contexts at an explicit stream boundary.
    pub fn reset(&mut self) {
        self.line.clear();
        self.states.clear();
        self.failed_closed = false;
    }

    fn process_line(&mut self, line: &str) -> Vec<String> {
        if self.failed_closed {
            return Vec::new();
        }
        if line.len() > MAX_STREAM_LINE_BYTES {
            self.fail_closed();
            return vec![REDACTED.to_owned()];
        }

        let buildkit = split_buildkit_line(line);
        if buildkit.step.is_none() && self.states.keys().any(Option::is_some) {
            // An unframed record cannot be assigned to any active BuildKit
            // step. Keep it suppressed until its owner emits a verified
            // envelope record again.
            return Vec::new();
        }
        if buildkit.step.is_some() && self.states.contains_key(&None) {
            // An unframed secret context has no verified owner to which a
            // BuildKit record can be attached. Suppress the record and leave
            // the unframed context open.
            return Vec::new();
        }
        let key = buildkit.step;
        let mut mode = self.states.remove(&key).unwrap_or(StreamMode::Normal);
        let output = self.process_record(buildkit.payload, buildkit.prefix, &mut mode);
        if !matches!(&mode, StreamMode::Normal) {
            if !self.states.contains_key(&key) && self.states.len() >= MAX_ACTIVE_ENVELOPES {
                self.fail_closed();
                return vec![REDACTED.to_owned()];
            }
            self.states.insert(key, mode);
        }
        output
    }

    fn process_record(
        &mut self,
        line: &str,
        line_prefix: &str,
        mode: &mut StreamMode,
    ) -> Vec<String> {
        let current = std::mem::replace(mode, StreamMode::Normal);
        match current {
            StreamMode::Normal => self.process_normal_line(line, line_prefix, mode),
            StreamMode::Pem => {
                if let Some(end) = private_key_footer_end(line) {
                    self.process_suffix(&line[end..], line_prefix, mode)
                } else {
                    *mode = StreamMode::Pem;
                    Vec::new()
                }
            }
            StreamMode::Block(mut block) => {
                if line.trim().is_empty() {
                    *mode = StreamMode::Block(block);
                    Vec::new()
                } else if let Some(indent) = leading_spaces(line) {
                    match block.body_indent {
                        Some(body_indent) if indent >= body_indent => {
                            *mode = StreamMode::Block(block);
                            Vec::new()
                        }
                        Some(_) if indent > block.header_indent => {
                            // A line that is indented under the secret key but
                            // falls short of the scalar's inferred or explicit
                            // body indentation is malformed or ambiguous. Do
                            // not treat it as a safe sibling record.
                            *mode = StreamMode::FailedClosed;
                            Vec::new()
                        }
                        Some(_) => self.process_normal_line(line, line_prefix, mode),
                        None if indent > block.header_indent => {
                            block.body_indent = Some(indent);
                            *mode = StreamMode::Block(block);
                            Vec::new()
                        }
                        None => self.process_normal_line(line, line_prefix, mode),
                    }
                } else {
                    *mode = StreamMode::FailedClosed;
                    Vec::new()
                }
            }
            StreamMode::Indented(mut block) => {
                if line.trim().is_empty() {
                    *mode = StreamMode::Indented(block);
                    Vec::new()
                } else if let Some(indent) = leading_spaces(line) {
                    match block.body_indent {
                        Some(body_indent) if indent >= body_indent => {
                            *mode = StreamMode::Indented(block);
                            Vec::new()
                        }
                        Some(_) if indent > block.header_indent => {
                            *mode = StreamMode::FailedClosed;
                            Vec::new()
                        }
                        Some(_) => self.process_normal_line(line, line_prefix, mode),
                        None if indent > block.header_indent => {
                            block.body_indent = Some(indent);
                            *mode = StreamMode::Indented(block);
                            Vec::new()
                        }
                        None => self.process_normal_line(line, line_prefix, mode),
                    }
                } else {
                    *mode = StreamMode::FailedClosed;
                    Vec::new()
                }
            }
            StreamMode::Quoted(mut quoted) => {
                if secret_assignment(line).is_some() {
                    // A second secret opener inside an unfinished quote is
                    // ambiguous: it may be a repeated BuildKit step id from
                    // another producer sharing this pipe. Never let it close
                    // the existing value by treating its opening quote as a
                    // delimiter.
                    *mode = StreamMode::FailedClosed;
                    return Vec::new();
                }
                if let Some(end) = quoted_value_end(line.as_bytes(), 0, &mut quoted) {
                    let suffix = &line[end..];
                    if safe_secret_suffix(suffix, quoted.allow_json_suffix) {
                        self.process_suffix(suffix, line_prefix, mode)
                    } else {
                        *mode = StreamMode::FailedClosed;
                        Vec::new()
                    }
                } else {
                    *mode = StreamMode::Quoted(quoted);
                    Vec::new()
                }
            }
            StreamMode::Structured(mut structured) => {
                if let Some(end) = structured_value_end(line.as_bytes(), 0, &mut structured) {
                    let suffix = &line[end..];
                    if safe_secret_suffix(suffix, true) {
                        self.process_suffix(suffix, line_prefix, mode)
                    } else {
                        *mode = StreamMode::FailedClosed;
                        Vec::new()
                    }
                } else if structured.malformed {
                    *mode = StreamMode::FailedClosed;
                    Vec::new()
                } else {
                    *mode = StreamMode::Structured(structured);
                    Vec::new()
                }
            }
            StreamMode::FailedClosed => {
                *mode = StreamMode::FailedClosed;
                Vec::new()
            }
        }
    }

    fn process_normal_line(
        &mut self,
        line: &str,
        line_prefix: &str,
        mode: &mut StreamMode,
    ) -> Vec<String> {
        let mut output = line_prefix.to_owned();
        let mut cursor = 0;
        while cursor < line.len() {
            let remaining = &line[cursor..];
            let pem_begin = private_key_begin(remaining);
            let assignment = secret_assignment(remaining);
            let pem_precedes_assignment = match (pem_begin, assignment) {
                (Some(begin), Some(assignment)) => begin < assignment.start,
                (Some(_), None) => true,
                _ => false,
            };
            if pem_precedes_assignment {
                let begin = pem_begin.expect("PEM marker precedes any assignment");
                output.push_str(&redact_unstructured_text(&remaining[..begin]));
                if let Some(end) = private_key_footer_end(&remaining[begin..]) {
                    output.push_str(REDACTED);
                    cursor += begin + end;
                    continue;
                }
                output.push_str(REDACTED);
                *mode = StreamMode::Pem;
                return vec![output];
            }
            let Some(assignment) = assignment else {
                output.push_str(&redact_unstructured_text(remaining));
                break;
            };
            output.push_str(&redact_unstructured_text(&remaining[..assignment.start]));
            let bytes = remaining.as_bytes();
            let value_start = assignment.value_start;
            if value_start >= bytes.len()
                || is_yaml_comment(bytes, value_start, assignment.separator)
            {
                output.push_str(REDACTED);
                *mode = StreamMode::FailedClosed;
                return vec![output];
            }

            let value = bytes[value_start];
            if assignment.is_authorization && is_bare_authorization_scheme(bytes, value_start) {
                output.push_str(REDACTED);
                *mode = StreamMode::FailedClosed;
                return vec![output];
            }
            if matches!(value, b'|' | b'>') && assignment.separator == b':' {
                let Some(explicit_indent) = yaml_block_indent(bytes, value_start) else {
                    output.push_str(REDACTED);
                    *mode = StreamMode::FailedClosed;
                    return vec![output];
                };
                let header_indent = leading_spaces(line).unwrap_or(0);
                *mode = StreamMode::Block(BlockState {
                    header_indent,
                    body_indent: explicit_indent.map(|increment| header_indent + increment),
                });
                output.push_str(REDACTED);
                return vec![output];
            }

            if matches!(value, b'\'' | b'"') {
                let delimiter_len = quote_delimiter_len(bytes, value_start);
                let allow_json_suffix = remaining[..assignment.start].contains('{')
                    || remaining[..assignment.start].contains('[');
                let mut quoted = QuotedState {
                    quote: value,
                    delimiter_len,
                    escaped: false,
                    allow_json_suffix,
                };
                if let Some(end) = quoted_value_end(bytes, value_start + delimiter_len, &mut quoted)
                {
                    if !safe_secret_suffix(&remaining[end..], allow_json_suffix) {
                        output.push_str(REDACTED);
                        *mode = StreamMode::FailedClosed;
                        return vec![output];
                    }
                    output.push_str(REDACTED);
                    cursor += end;
                    continue;
                }
                *mode = StreamMode::Quoted(quoted);
                output.push_str(REDACTED);
                return vec![output];
            }

            if matches!(value, b'{' | b'[') {
                let mut structured = StructuredState {
                    expected_closers: Vec::new(),
                    in_string: None,
                    escaped: false,
                    malformed: false,
                };
                if let Some(end) = structured_value_end(bytes, value_start, &mut structured) {
                    if !safe_secret_suffix(&remaining[end..], true) {
                        output.push_str(REDACTED);
                        *mode = StreamMode::FailedClosed;
                        return vec![output];
                    }
                    output.push_str(REDACTED);
                    cursor += end;
                    continue;
                }
                output.push_str(REDACTED);
                if structured.malformed {
                    *mode = StreamMode::FailedClosed;
                } else {
                    *mode = StreamMode::Structured(structured);
                }
                return vec![output];
            }

            if let Some(begin) = private_key_begin(&remaining[value_start..]) {
                let begin = value_start + begin;
                output.push_str(REDACTED);
                if let Some(end) = private_key_footer_end(&remaining[begin..]) {
                    cursor += begin + end;
                    continue;
                }
                *mode = StreamMode::Pem;
                return vec![output];
            }

            let end = plain_value_end(
                remaining,
                value_start,
                assignment.separator,
                assignment.is_authorization,
            );
            output.push_str(REDACTED);
            if assignment.separator == b':'
                && !remaining[..assignment.start].contains('{')
                && !remaining[..assignment.start].contains('"')
            {
                *mode = StreamMode::Indented(IndentedState {
                    header_indent: leading_spaces(line).unwrap_or(0),
                    body_indent: None,
                });
                return vec![output];
            }
            cursor += end;
        }
        vec![output]
    }

    fn process_suffix(
        &mut self,
        suffix: &str,
        line_prefix: &str,
        mode: &mut StreamMode,
    ) -> Vec<String> {
        if suffix.is_empty() {
            Vec::new()
        } else {
            self.process_normal_line(suffix, line_prefix, mode)
        }
    }

    fn fail_closed(&mut self) {
        self.line.clear();
        self.states.clear();
        self.failed_closed = true;
    }
}

/// Redact credentials and key material in a complete text value.
pub fn redact_text(input: &str) -> Cow<'_, str> {
    let mut redactor = StreamRedactor::default();
    let mut redacted = redactor.push_complete_text(input);
    let _ = redactor.finish();
    if !input.ends_with('\n') {
        if let Some(without_crlf) = redacted.strip_suffix("\r\n") {
            redacted = without_crlf.to_owned();
        } else if let Some(without_lf) = redacted.strip_suffix('\n') {
            redacted = without_lf.to_owned();
        }
    }
    if redacted.as_str() == input {
        Cow::Borrowed(input)
    } else {
        Cow::Owned(redacted)
    }
}

pub fn redact_and_cap(input: &str, max_bytes: usize) -> String {
    let redacted = redact_text(input);
    cap_text(redacted.as_ref(), max_bytes)
}

fn cap_text(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_owned();
    }

    let prefix = format!("(truncated to {max_bytes} bytes)\n");
    let retained = max_bytes.saturating_sub(prefix.len());
    if retained == 0 {
        return prefix.chars().take(max_bytes).collect();
    }
    let mut start = input.len() - retained;
    while !input.is_char_boundary(start) {
        start += 1;
    }
    format!("{prefix}{}", &input[start..])
}

fn redaction_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            r"\bgithub_pat_[A-Za-z0-9_]{20,}\b",
            r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b",
            r"\bsk-[A-Za-z0-9_-]{20,}\b",
            r"\bxox[bpars]-[A-Za-z0-9-]{20,}\b",
            r"\bAKIA[0-9A-Z]{16}\b",
            r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b",
            r"(?i)(?:[:=]\s*)[A-F0-9]{32,}\b",
            r"(?:[:=]\s*)[A-Za-z0-9+/]{40,}={0,2}\b",
        ]
        .into_iter()
        .map(|pattern| match Regex::new(pattern) {
            Ok(regex) => regex,
            Err(error) => unreachable!("valid diagnostics redaction regex: {error}"),
        })
        .collect()
    })
}

fn redact_unstructured_text(input: &str) -> Cow<'_, str> {
    let mut current = Cow::Borrowed(input);
    for regex in redaction_patterns() {
        if regex.is_match(&current) {
            current = Cow::Owned(regex.replace_all(&current, REDACTED).into_owned());
        }
    }
    current
}

fn append_record(output: &mut String, lines: Vec<String>, ending: &str) {
    if lines.is_empty() {
        return;
    }
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        output.push_str(line);
    }
    output.push_str(ending);
}

fn secret_assignment(line: &str) -> Option<Assignment> {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    let matcher = MATCHER.get_or_init(|| {
        Regex::new(
            r#"(?i)["']?[A-Z0-9_-]*(?:authorization|bearer|token|secret|password|passwd|credential|api[_-]?key|access[_-]?key|private[_-]?key)[A-Z0-9_-]*["']?\s*[:=]\s*"#,
        )
        .expect("valid secret assignment matcher")
    });
    let found = matcher.find(line)?;
    let matched = &line[found.start()..found.end()];
    let separator_offset = matched.find(|character| matches!(character, ':' | '='))?;
    let key = &matched[..separator_offset];
    Some(Assignment {
        start: found.start(),
        value_start: found.end(),
        separator: matched.as_bytes()[separator_offset],
        is_authorization: key.to_ascii_lowercase().contains("authorization"),
    })
}

fn split_buildkit_line(line: &str) -> BuildKitLine<'_> {
    let bytes = line.as_bytes();
    if bytes.first() != Some(&b'#') {
        return BuildKitLine {
            prefix: "",
            payload: line,
            step: None,
        };
    }

    let mut index = 1;
    let step_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if index == step_start
        || index - step_start > 20
        || !bytes.get(index).is_some_and(u8::is_ascii_whitespace)
    {
        return BuildKitLine {
            prefix: "",
            payload: line,
            step: None,
        };
    }
    let step_end = index;
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }

    let timestamp_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if index == timestamp_start || bytes.get(index) != Some(&b'.') {
        return BuildKitLine {
            prefix: "",
            payload: line,
            step: None,
        };
    }
    index += 1;
    let fraction_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if index == fraction_start || !bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        return BuildKitLine {
            prefix: "",
            payload: line,
            step: None,
        };
    }
    index += 1;

    let normalized_step = line[step_start..step_end].trim_start_matches('0');
    let normalized_step = if normalized_step.is_empty() {
        "0"
    } else {
        normalized_step
    };
    BuildKitLine {
        prefix: &line[..index],
        payload: &line[index..],
        step: Some(normalized_step.to_owned()),
    }
}

fn is_yaml_comment(bytes: &[u8], value_start: usize, separator: u8) -> bool {
    separator == b':' && bytes.get(value_start) == Some(&b'#')
}

fn is_bare_authorization_scheme(bytes: &[u8], value_start: usize) -> bool {
    let value = &bytes[value_start..];
    let scheme_end = value
        .iter()
        .position(u8::is_ascii_whitespace)
        .unwrap_or(value.len());
    let scheme = &value[..scheme_end];
    (scheme.eq_ignore_ascii_case(b"bearer") || scheme.eq_ignore_ascii_case(b"basic"))
        && value[scheme_end..].iter().all(u8::is_ascii_whitespace)
}

fn safe_secret_suffix(suffix: &str, allow_json_separators: bool) -> bool {
    let Some(offset) = suffix.bytes().position(|byte| !byte.is_ascii_whitespace()) else {
        return true;
    };
    let first = suffix.as_bytes()[offset];
    if offset > 0 && first == b'#' {
        return true;
    }
    if !allow_json_separators {
        return false;
    }
    if first == b',' {
        let remainder = suffix[offset + 1..].trim_start();
        return remainder.is_empty()
            || remainder.starts_with('}')
            || remainder.starts_with(']')
            || remainder.starts_with('{')
            || remainder.starts_with('[')
            || remainder.starts_with('"')
            || remainder.starts_with('\'')
            || remainder.starts_with('-')
            || remainder.starts_with(|character: char| character.is_ascii_digit())
            || remainder.starts_with("true")
            || remainder.starts_with("false")
            || remainder.starts_with("null");
    }
    if matches!(first, b'}' | b']') {
        let after = &suffix[offset + 1..];
        let remainder = after.trim_start();
        return remainder.is_empty()
            || (remainder.starts_with('#') && remainder.len() < after.len())
            || remainder.starts_with(',')
            || remainder.starts_with('}')
            || remainder.starts_with(']');
    }
    false
}

/// Return the explicit YAML block indentation increment, or `None` when the
/// scalar uses the first content line to infer indentation. An outer `None`
/// means the indicator is malformed and must fail closed.
fn yaml_block_indent(bytes: &[u8], start: usize) -> Option<Option<usize>> {
    let mut index = start + 1;
    let mut has_chomping = false;
    if bytes
        .get(index)
        .is_some_and(|byte| *byte == b'+' || *byte == b'-')
    {
        has_chomping = true;
        index += 1;
    }
    let mut explicit_indent = None;
    if bytes
        .get(index)
        .is_some_and(|byte| (b'1'..=b'9').contains(byte))
    {
        explicit_indent = Some(usize::from(bytes[index] - b'0'));
        index += 1;
    }
    if !has_chomping
        && bytes
            .get(index)
            .is_some_and(|byte| *byte == b'+' || *byte == b'-')
    {
        index += 1;
    }
    while bytes.get(index) == Some(&b' ') {
        index += 1;
    }
    if index < bytes.len()
        && (bytes.get(index) != Some(&b'#')
            || index == start + 1
            || !bytes[index - 1].is_ascii_whitespace())
    {
        return None;
    }
    Some(explicit_indent)
}

fn plain_value_end(line: &str, start: usize, separator: u8, is_authorization: bool) -> usize {
    let bytes = line.as_bytes();
    if is_authorization {
        return bytes.len();
    }
    if separator == b':' {
        let json_like = line[..start].contains('{') || line[..start].contains('"');
        for (offset, &byte) in bytes[start..].iter().enumerate() {
            if (json_like && matches!(byte, b',' | b'}' | b']'))
                || (byte == b'#' && offset > 0 && bytes[start + offset - 1].is_ascii_whitespace())
            {
                return start + offset;
            }
        }
        return bytes.len();
    }
    for (offset, &byte) in bytes[start..].iter().enumerate() {
        if byte.is_ascii_whitespace() || matches!(byte, b',' | b'}' | b']') {
            return start + offset;
        }
    }
    bytes.len()
}

fn quote_delimiter_len(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let count = bytes[start..]
        .iter()
        .take_while(|byte| **byte == quote)
        .count();
    if count >= 3 { 3 } else { 1 }
}

fn quoted_value_end(bytes: &[u8], start: usize, state: &mut QuotedState) -> Option<usize> {
    let mut index = start;
    while index < bytes.len() {
        let byte = bytes[index];
        if state.escaped {
            state.escaped = false;
        } else if byte == b'\\' {
            state.escaped = true;
        } else if byte == state.quote {
            let quote_count = bytes[index..]
                .iter()
                .take_while(|quote| **quote == state.quote)
                .count();
            if state.delimiter_len == 1 && state.quote == b'\'' && quote_count >= 2 {
                // YAML single-quoted scalars escape a quote by doubling it.
                index += 2;
                continue;
            }
            if quote_count >= state.delimiter_len {
                return Some(index + state.delimiter_len);
            }
        }
        index += 1;
    }
    // A physical newline is part of a multiline quoted scalar; retain a
    // trailing escape so a following quote cannot accidentally end the span.
    None
}

fn structured_value_end(bytes: &[u8], start: usize, state: &mut StructuredState) -> Option<usize> {
    for (offset, &byte) in bytes[start..].iter().enumerate() {
        let index = start + offset;
        if let Some(quote) = state.in_string {
            if state.escaped {
                state.escaped = false;
            } else if byte == b'\\' {
                state.escaped = true;
            } else if byte == quote {
                if quote == b'\'' && bytes.get(index + 1) == Some(&quote) {
                    state.escaped = true;
                    continue;
                }
                state.in_string = None;
            }
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            state.in_string = Some(byte);
        } else if matches!(byte, b'{' | b'[') {
            if state.expected_closers.len() >= MAX_STRUCTURED_DEPTH {
                state.malformed = true;
                return None;
            }
            state
                .expected_closers
                .push(if byte == b'{' { b'}' } else { b']' });
        } else if matches!(byte, b'}' | b']') {
            if state.expected_closers.pop() != Some(byte) {
                state.malformed = true;
                return None;
            }
            if state.expected_closers.is_empty() {
                return Some(index + 1);
            }
        }
    }
    None
}

fn private_key_begin(input: &str) -> Option<usize> {
    private_key_begin_matcher()
        .find(input)
        .map(|found| found.start())
}

fn private_key_footer_end(input: &str) -> Option<usize> {
    private_key_footer_matcher()
        .find(input)
        .map(|found| found.end())
}

fn private_key_begin_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----")
            .expect("valid private key begin matcher")
    })
}

fn private_key_footer_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"-----END [A-Z0-9 ]*PRIVATE KEY-----")
            .expect("valid private key footer matcher")
    })
}

fn leading_spaces(line: &str) -> Option<usize> {
    let mut indent = 0;
    for byte in line.bytes() {
        match byte {
            b' ' => indent += 1,
            b'\t' => return None,
            _ => break,
        }
    }
    Some(indent)
}

#[cfg(test)]
mod tests;
