use std::borrow::Cow;
use std::sync::OnceLock;

use regex::Regex;

const REDACTED: &str = "<redacted>";
const MAX_STREAM_LINE_BYTES: usize = 64 * 1024;
const MAX_STRUCTURED_DEPTH: usize = 256;

/// Incremental redactor for one output stream.
///
/// The caller must use a distinct instance for stdout and stderr. Lines are
/// held until their newline arrives, and incomplete structured values are
/// suppressed until their closing delimiter. A malformed or overlong stream
/// fails closed by suppressing the remainder of that stream.
#[derive(Debug)]
pub struct StreamRedactor {
    line: Vec<u8>,
    mode: StreamMode,
}

impl Default for StreamRedactor {
    fn default() -> Self {
        Self {
            line: Vec::new(),
            mode: StreamMode::Normal,
        }
    }
}

#[derive(Debug)]
enum StreamMode {
    Normal,
    Pem { step: Option<String> },
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
    step: Option<String>,
}

#[derive(Debug)]
struct IndentedState {
    header_indent: usize,
    body_indent: Option<usize>,
    step: Option<String>,
}

#[derive(Debug)]
struct QuotedState {
    quote: u8,
    delimiter_len: usize,
    escaped: bool,
    step: Option<String>,
}

#[derive(Debug)]
struct StructuredState {
    expected_closers: Vec<u8>,
    in_string: Option<u8>,
    escaped: bool,
    malformed: bool,
    step: Option<String>,
}

#[derive(Clone, Copy)]
struct BuildKitLine<'a> {
    prefix: &'a str,
    payload: &'a str,
    step: Option<&'a str>,
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
            if matches!(&self.mode, StreamMode::FailedClosed) {
                continue;
            }
            if byte == b'\n' {
                let line = String::from_utf8_lossy(&self.line).into_owned();
                let line = line.trim_end_matches('\r');
                output.extend(self.process_line(line));
                self.line.clear();
            } else if self.line.len() < MAX_STREAM_LINE_BYTES {
                self.line.push(byte);
            } else {
                // Once a physical line is too large to inspect in bounded
                // memory, suppress it and the rest of this stream. It may be
                // the continuation of a sensitive value.
                self.line.clear();
                self.mode = StreamMode::FailedClosed;
                output.push(REDACTED.to_owned());
            }
        }
        output
    }

    /// Finish the stream. An unterminated line is redacted as a complete
    /// final line; any open secret construct has already been represented by
    /// a redaction marker and remains fail-closed.
    pub fn finish(&mut self) -> Vec<String> {
        let mut output = Vec::new();
        if !self.line.is_empty() && !matches!(&self.mode, StreamMode::FailedClosed) {
            let line = String::from_utf8_lossy(&self.line).into_owned();
            output.extend(self.process_line(line.trim_end_matches('\r')));
        }
        self.line.clear();
        self.mode = StreamMode::Normal;
        output
    }

    fn process_line(&mut self, line: &str) -> Vec<String> {
        let buildkit = split_buildkit_line(line);
        if let Some(expected_step) = active_step(&self.mode)
            && expected_step.as_deref() != buildkit.step
        {
            // BuildKit interleaves records from separate steps on one pipe.
            // A record from another step cannot close this secret's envelope;
            // suppress it and keep the open state for its owning step.
            return Vec::new();
        }
        let mode = std::mem::replace(&mut self.mode, StreamMode::Normal);
        match mode {
            StreamMode::Normal => self.process_normal_line(
                buildkit.payload,
                buildkit.prefix,
                buildkit.step.map(str::to_owned),
            ),
            StreamMode::Pem { step } => {
                if let Some(end) = private_key_footer_end(buildkit.payload) {
                    self.process_suffix(
                        &buildkit.payload[end..],
                        buildkit.prefix,
                        buildkit.step.map(str::to_owned),
                    )
                } else {
                    self.mode = StreamMode::Pem { step };
                    Vec::new()
                }
            }
            StreamMode::Block(mut block) => {
                if buildkit.payload.trim().is_empty() {
                    self.mode = StreamMode::Block(block);
                    Vec::new()
                } else {
                    let indent = leading_spaces(buildkit.payload);
                    match block.body_indent {
                        Some(body_indent) if indent >= body_indent => {
                            self.mode = StreamMode::Block(block);
                            Vec::new()
                        }
                        Some(_) => self.process_normal_line(
                            buildkit.payload,
                            buildkit.prefix,
                            buildkit.step.map(str::to_owned),
                        ),
                        None if indent > block.header_indent => {
                            block.body_indent = Some(indent);
                            self.mode = StreamMode::Block(block);
                            Vec::new()
                        }
                        None => {
                            // A block header with no provable body indentation
                            // is ambiguous. Keep the stream closed rather than
                            // treating a possibly misframed value as safe.
                            self.mode = StreamMode::FailedClosed;
                            Vec::new()
                        }
                    }
                }
            }
            StreamMode::Indented(mut block) => {
                if buildkit.payload.trim().is_empty() {
                    self.mode = StreamMode::Indented(block);
                    Vec::new()
                } else {
                    let indent = leading_spaces(buildkit.payload);
                    match block.body_indent {
                        Some(body_indent) if indent >= body_indent => {
                            self.mode = StreamMode::Indented(block);
                            Vec::new()
                        }
                        Some(_) => self.process_normal_line(
                            buildkit.payload,
                            buildkit.prefix,
                            buildkit.step.map(str::to_owned),
                        ),
                        None if indent > block.header_indent => {
                            block.body_indent = Some(indent);
                            self.mode = StreamMode::Indented(block);
                            Vec::new()
                        }
                        None => {
                            self.mode = StreamMode::FailedClosed;
                            Vec::new()
                        }
                    }
                }
            }
            StreamMode::Quoted(mut quoted) => {
                if let Some(end) =
                    quoted_value_end(buildkit.payload.as_bytes(), 0, &mut quoted)
                {
                    self.process_suffix(
                        &buildkit.payload[end..],
                        buildkit.prefix,
                        buildkit.step.map(str::to_owned),
                    )
                } else {
                    self.mode = StreamMode::Quoted(quoted);
                    Vec::new()
                }
            }
            StreamMode::Structured(mut structured) => {
                if let Some(end) =
                    structured_value_end(buildkit.payload.as_bytes(), 0, &mut structured)
                {
                    self.process_suffix(
                        &buildkit.payload[end..],
                        buildkit.prefix,
                        buildkit.step.map(str::to_owned),
                    )
                } else if structured.malformed {
                    self.mode = StreamMode::FailedClosed;
                    Vec::new()
                } else {
                    self.mode = StreamMode::Structured(structured);
                    Vec::new()
                }
            }
            StreamMode::FailedClosed => {
                self.mode = StreamMode::FailedClosed;
                Vec::new()
            }
        }
    }

    fn process_normal_line(
        &mut self,
        line: &str,
        line_prefix: &str,
        step: Option<String>,
    ) -> Vec<String> {
        if let Some(begin) = private_key_begin(line) {
            if let Some(end) = private_key_footer_end(&line[begin..]) {
                let end = begin + end;
                let mut output = line_prefix.to_owned();
                output.push_str(&redact_text(&line[..begin]));
                output.push_str(REDACTED);
                if !line[end..].is_empty() {
                    output.push_str(
                        &self
                            .process_normal_line(&line[end..], "", step)
                            .join("\n"),
                    );
                }
                return vec![output];
            }
            self.mode = StreamMode::Pem { step };
            return vec![format!(
                "{line_prefix}{}{REDACTED}",
                redact_prefix(&line[..begin])
            )];
        }

        let mut output = line_prefix.to_owned();
        let mut cursor = 0;
        while cursor < line.len() {
            let remaining = &line[cursor..];
            let Some(assignment) = secret_assignment(remaining) else {
                output.push_str(&redact_text(remaining));
                break;
            };
            output.push_str(&redact_text(&remaining[..assignment.start]));
            let bytes = remaining.as_bytes();
            let value_start = assignment.value_start;
            if value_start >= bytes.len()
                || is_yaml_comment(bytes, value_start, assignment.separator)
            {
                self.mode = StreamMode::Indented(IndentedState {
                    header_indent: leading_spaces(line),
                    body_indent: None,
                    step: step.clone(),
                });
                output.push_str(REDACTED);
                return vec![output];
            }

            let value = bytes[value_start];
            if assignment.is_authorization && is_bare_bearer(bytes, value_start) {
                output.push_str(REDACTED);
                self.mode = StreamMode::FailedClosed;
                return vec![output];
            }
            if matches!(value, b'|' | b'>')
                && is_yaml_block_indicator(bytes, value_start, assignment.separator)
            {
                self.mode = StreamMode::Block(BlockState {
                    header_indent: leading_spaces(line),
                    body_indent: None,
                    step: step.clone(),
                });
                output.push_str(REDACTED);
                return vec![output];
            }

            if matches!(value, b'\'' | b'"') {
                let delimiter_len = quote_delimiter_len(bytes, value_start);
                let mut quoted = QuotedState {
                    quote: value,
                    delimiter_len,
                    escaped: false,
                    step: step.clone(),
                };
                if let Some(end) =
                    quoted_value_end(bytes, value_start + delimiter_len, &mut quoted)
                {
                    output.push_str(REDACTED);
                    cursor += end;
                    continue;
                }
                self.mode = StreamMode::Quoted(quoted);
                output.push_str(REDACTED);
                return vec![output];
            }

            if matches!(value, b'{' | b'[') {
                let mut structured = StructuredState {
                    expected_closers: Vec::new(),
                    in_string: None,
                    escaped: false,
                    malformed: false,
                    step: step.clone(),
                };
                if let Some(end) = structured_value_end(bytes, value_start, &mut structured) {
                    output.push_str(REDACTED);
                    cursor += end;
                    continue;
                }
                output.push_str(REDACTED);
                if structured.malformed {
                    self.mode = StreamMode::FailedClosed;
                } else {
                    self.mode = StreamMode::Structured(structured);
                }
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
                self.mode = StreamMode::Indented(IndentedState {
                    header_indent: leading_spaces(line),
                    body_indent: None,
                    step,
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
        step: Option<String>,
    ) -> Vec<String> {
        if suffix.is_empty() {
            Vec::new()
        } else {
            self.process_normal_line(suffix, line_prefix, step)
        }
    }
}

/// Redact credentials and key material in a complete text value.
pub fn redact_text(input: &str) -> Cow<'_, str> {
    let mut current: Cow<'_, str> = Cow::Borrowed(input);
    for regex in redaction_patterns() {
        if regex.is_match(&current) {
            current = Cow::Owned(regex.replace_all(&current, REDACTED).into_owned());
        }
    }
    if let Some(start) = unclosed_private_key_start(current.as_ref()) {
        let mut redacted = current[..start].to_owned();
        redacted.push_str(REDACTED);
        current = Cow::Owned(redacted);
    }
    current
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
            r"(?is)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----",
            r"(?i)\bauthorization\b\s*[:=]\s*bearer\s+[^\s,'\x22}\]]+",
            r#"(?is)["']?[A-Z0-9_-]*(?:authorization|bearer|token|secret|password|passwd|credential|api[_-]?key|access[_-]?key|private[_-]?key)[A-Z0-9_-]*["']?\s*[:=]\s*(?:"{3}.*?(?:"{3}|$)|'{3}.*?(?:'{3}|$)|"(?:\\.|[^"])*(?:"|$)|'(?:\\.|[^'])*(?:'|$)|[^\s,'"}\]]+)"#,
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
    if index == step_start || !bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
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

    BuildKitLine {
        prefix: &line[..index],
        payload: &line[index..],
        step: Some(&line[step_start..step_end]),
    }
}

fn active_step(mode: &StreamMode) -> Option<&Option<String>> {
    match mode {
        StreamMode::Normal | StreamMode::FailedClosed => None,
        StreamMode::Pem { step }
        | StreamMode::Block(BlockState { step, .. })
        | StreamMode::Indented(IndentedState { step, .. })
        | StreamMode::Quoted(QuotedState { step, .. })
        | StreamMode::Structured(StructuredState { step, .. }) => Some(step),
    }
}

fn is_yaml_comment(bytes: &[u8], value_start: usize, separator: u8) -> bool {
    separator == b':' && bytes.get(value_start) == Some(&b'#')
}

fn is_bare_bearer(bytes: &[u8], value_start: usize) -> bool {
    let value = &bytes[value_start..];
    value
        .get(..6)
        .is_some_and(|word| word.eq_ignore_ascii_case(b"Bearer"))
        && value[6..].iter().all(u8::is_ascii_whitespace)
}

fn is_yaml_block_indicator(bytes: &[u8], start: usize, separator: u8) -> bool {
    if separator != b':' {
        return false;
    }
    let mut index = start + 1;
    if bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    while bytes
        .get(index)
        .is_some_and(|byte| *byte == b'+' || *byte == b'-')
    {
        index += 1;
    }
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    index == bytes.len() || bytes.get(index) == Some(&b'#')
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
            if state.delimiter_len == 1
                && state.quote == b'\''
                && quote_count >= 2
            {
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

fn structured_value_end(
    bytes: &[u8],
    start: usize,
    state: &mut StructuredState,
) -> Option<usize> {
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
            state.expected_closers.push(if byte == b'{' { b'}' } else { b']' });
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
    private_key_begin_matcher().find(input).map(|found| found.start())
}

fn private_key_footer_end(input: &str) -> Option<usize> {
    private_key_footer_matcher().find(input).map(|found| found.end())
}

fn unclosed_private_key_start(input: &str) -> Option<usize> {
    let start = private_key_begin(input)?;
    private_key_footer_end(&input[start..]).is_none().then_some(start)
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

fn leading_spaces(line: &str) -> usize {
    line.bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count()
}

fn redact_prefix(prefix: &str) -> String {
    redact_text(prefix).into_owned()
}

#[cfg(test)]
mod tests;
