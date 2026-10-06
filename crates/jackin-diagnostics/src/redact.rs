use std::borrow::Cow;
use std::sync::OnceLock;

use regex::Regex;

const REDACTED: &str = "<redacted>";
pub fn redact_text(input: &str) -> Cow<'_, str> {
    let mut current = redact_secret_assignments(input);
    if let Cow::Owned(redacted) = redact_private_key_blocks(current.as_ref()) {
        current = Cow::Owned(redacted);
    }
    for regex in redaction_patterns() {
        if regex.is_match(&current) {
            current = Cow::Owned(regex.replace_all(&current, REDACTED).into_owned());
        }
    }
    current
}

/// Return the start of an unfinished secret construct at the end of a stream
/// buffer. Callers must retain text from that line until more input arrives;
/// otherwise multiline credentials split across reads can leak.
#[doc(hidden)]
pub fn incomplete_secret_start(input: &str) -> Option<usize> {
    let assignment_start = secret_assignment_spans(input)
        .into_iter()
        .find_map(|span| (!span.closed).then_some(span.start));
    let private_key_start = match private_key_spans(input) {
        Some(spans) => spans
            .into_iter()
            .find_map(|span| (!span.closed).then_some(span.start)),
        None if !input.is_empty() => Some(0),
        None => None,
    };

    match (assignment_start, private_key_start) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(start), None) | (None, Some(start)) => Some(start),
        (None, None) => None,
    }
}

/// Number of bytes in complete lines that can be redacted and emitted safely.
/// Hold the line containing any unfinished span or any span crossing the
/// complete-line boundary until the entire sensitive value is available.
#[doc(hidden)]
pub fn safe_complete_prefix_len(input: &str) -> usize {
    let complete_end = input.rfind('\n').map_or(0, |index| index + 1);
    let crossing_start = secret_assignment_spans(input)
        .into_iter()
        .chain(private_key_spans(input).into_iter().flatten())
        .filter(|span| span.start < complete_end && complete_end < span.end)
        .map(|span| span.start)
        .min();
    let secret_start = incomplete_secret_start(input)
        .into_iter()
        .chain(crossing_start)
        .min();
    let Some(secret_start) = secret_start else {
        return complete_end;
    };
    input[..secret_start]
        .rfind('\n')
        .map_or(0, |index| index + 1)
        .min(complete_end)
}

#[derive(Clone, Copy)]
struct SecretAssignmentSpan {
    start: usize,
    end: usize,
    closed: bool,
}

struct YamlPlainScalarEnd {
    end: usize,
    closed: bool,
    split_at_comment: Option<(usize, usize)>,
}

fn redact_secret_assignments(input: &str) -> Cow<'_, str> {
    let spans = secret_assignment_spans(input);
    if spans.is_empty() {
        return Cow::Borrowed(input);
    }

    let mut output = String::with_capacity(input.len());
    let mut copied_through = 0;
    for span in spans {
        if span.start < copied_through {
            continue;
        }
        output.push_str(&input[copied_through..span.start]);
        output.push_str(REDACTED);
        copied_through = span.end;
    }
    output.push_str(&input[copied_through..]);
    Cow::Owned(output)
}

fn redact_private_key_blocks(input: &str) -> Cow<'_, str> {
    let Some(spans) = private_key_spans(input) else {
        return if input.is_empty() {
            Cow::Borrowed(input)
        } else {
            Cow::Owned(REDACTED.to_owned())
        };
    };
    if spans.is_empty() {
        return Cow::Borrowed(input);
    }

    let mut output = String::with_capacity(input.len());
    let mut copied_through = 0;
    for span in spans {
        if span.start < copied_through {
            continue;
        }
        output.push_str(&input[copied_through..span.start]);
        output.push_str(REDACTED);
        copied_through = span.end;
    }
    output.push_str(&input[copied_through..]);
    Cow::Owned(output)
}

fn private_key_spans(input: &str) -> Option<Vec<SecretAssignmentSpan>> {
    let matcher = private_key_matcher()?;
    let mut spans = Vec::new();
    for captures in matcher.captures_iter(input) {
        let whole = captures.get(0)?;
        let footer = captures.get(2)?;
        spans.push(SecretAssignmentSpan {
            start: whole.start(),
            end: whole.end(),
            closed: !footer.is_empty(),
        });
    }
    Some(spans)
}

fn secret_assignment_spans(input: &str) -> Vec<SecretAssignmentSpan> {
    let Some(matcher) = secret_key_matcher() else {
        return if input.is_empty() {
            Vec::new()
        } else {
            vec![SecretAssignmentSpan {
                start: 0,
                end: input.len(),
                closed: false,
            }]
        };
    };
    let mut spans = Vec::new();
    let mut candidates: Vec<_> = matcher
        .find_iter(input)
        .map(|candidate| (candidate.start(), candidate.end()))
        .collect();
    candidates.extend(ascii_secret_key_candidates(input));
    candidates.sort_unstable();
    candidates.dedup();

    let bytes = input.as_bytes();
    for (key_start, key_end) in candidates {
        if spans
            .iter()
            .any(|span: &SecretAssignmentSpan| span.start <= key_start && key_start < span.end)
        {
            continue;
        }

        let mut delimiter = key_end;
        consume_optional_quote(bytes, &mut delimiter);
        skip_whitespace(bytes, &mut delimiter);
        if delimiter == bytes.len() || !matches!(bytes[delimiter], b':' | b'=') {
            continue;
        }
        let is_yaml_mapping = bytes[delimiter] == b':';
        delimiter += 1;
        skip_whitespace(bytes, &mut delimiter);
        if delimiter == bytes.len() {
            spans.push(SecretAssignmentSpan {
                start: assignment_start(bytes, key_start),
                end: bytes.len(),
                closed: false,
            });
            break;
        }

        let start = assignment_start(bytes, key_start);
        let key = &input[key_start..key_end];
        let is_authorization = (key.is_ascii() && key_has_word(key, "authorization"))
            || authorization_key_matcher().is_some_and(|authorization| authorization.is_match(key));
        let mut yaml_split = None;
        let (end, closed) =
            if let Some((quote_start, quote_depth)) = quoted_value_start(bytes, delimiter) {
                quoted_value_end(input, quote_start, quote_depth)
            } else {
                let bounded = yaml_block_scalar_end(input, delimiter, start)
                    .or_else(|| private_key_value_end(input, delimiter))
                    .or_else(|| {
                        is_authorization.then(|| authorization_header_value_end(input, delimiter))
                    });
                match bounded {
                    Some(span) => span,
                    None => match yaml_plain_scalar_end(input, delimiter, start, is_yaml_mapping) {
                        Some(span) => {
                            yaml_split = span.split_at_comment;
                            (span.end, span.closed)
                        }
                        None => (unquoted_value_end(bytes, delimiter), true),
                    },
                }
            };
        if let Some((comment_start, continuation_start)) = yaml_split {
            spans.push(SecretAssignmentSpan {
                start,
                end: comment_start,
                closed,
            });
            if continuation_start < end {
                spans.push(SecretAssignmentSpan {
                    start: continuation_start,
                    end,
                    closed,
                });
            }
        } else {
            spans.push(SecretAssignmentSpan { start, end, closed });
        }
    }
    normalize_secret_assignment_spans(spans)
}

fn normalize_secret_assignment_spans(
    mut spans: Vec<SecretAssignmentSpan>,
) -> Vec<SecretAssignmentSpan> {
    spans.sort_unstable_by_key(|span| (span.start, span.end));
    let mut normalized: Vec<SecretAssignmentSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        if let Some(previous) = normalized
            .last_mut()
            .filter(|previous| span.start < previous.end)
        {
            previous.end = previous.end.max(span.end);
            previous.closed &= span.closed;
        } else {
            normalized.push(span);
        }
    }
    normalized
}

fn ascii_secret_key_candidates(input: &str) -> Vec<(usize, usize)> {
    let bytes = input.as_bytes();
    let mut candidates = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if !is_identifier_byte(bytes[cursor])
            || (cursor > 0 && is_identifier_byte(bytes[cursor - 1]))
        {
            cursor += 1;
            continue;
        }

        let start = cursor;
        while cursor < bytes.len() && is_identifier_byte(bytes[cursor]) {
            cursor += 1;
        }
        if is_secret_key(&input[start..cursor]) {
            candidates.push((start, cursor));
        }
    }
    candidates
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn is_secret_key(key: &str) -> bool {
    let words = identifier_words(key);
    words.iter().any(|word| {
        matches!(
            word.as_str(),
            "authorization" | "bearer" | "token" | "secret" | "password" | "passwd" | "credential"
        )
    }) || words.windows(2).any(|pair| {
        matches!(pair, [prefix, suffix] if matches!(prefix.as_str(), "api" | "access" | "private") && suffix == "key")
    }) || matches!(
        key.replace(['_', '-'], "").to_ascii_lowercase().as_str(),
        "apikey" | "accesskey" | "privatekey"
    )
}

fn key_has_word(key: &str, expected: &str) -> bool {
    identifier_words(key).iter().any(|word| word == expected)
}

fn identifier_words(key: &str) -> Vec<String> {
    let bytes = key.as_bytes();
    let mut words = Vec::new();
    let mut word_start = 0;
    for index in 0..bytes.len() {
        if matches!(bytes[index], b'_' | b'-') {
            if word_start < index {
                words.push(key[word_start..index].to_ascii_lowercase());
            }
            word_start = index + 1;
            continue;
        }
        if index > word_start
            && bytes[index].is_ascii_uppercase()
            && (bytes[index - 1].is_ascii_lowercase()
                || bytes[index - 1].is_ascii_digit()
                || (bytes[index - 1].is_ascii_uppercase()
                    && bytes.get(index + 1).is_some_and(u8::is_ascii_lowercase)))
        {
            words.push(key[word_start..index].to_ascii_lowercase());
            word_start = index;
        }
    }
    if word_start < bytes.len() {
        words.push(key[word_start..].to_ascii_lowercase());
    }
    words
}

fn secret_key_matcher() -> Option<&'static Regex> {
    static SECRET_KEY: OnceLock<Option<Regex>> = OnceLock::new();
    SECRET_KEY
        .get_or_init(|| {
            Regex::new(
                r"(?i)\b(?:authorization|bearer|token|secret|password|passwd|credential|api[_-]?key|access[_-]?key|private[_-]?key)\b",
            )
            .ok()
        })
        .as_ref()
}

fn authorization_key_matcher() -> Option<&'static Regex> {
    static AUTHORIZATION_KEY: OnceLock<Option<Regex>> = OnceLock::new();
    AUTHORIZATION_KEY
        .get_or_init(|| Regex::new(r"(?i)^authorization$").ok())
        .as_ref()
}

fn assignment_start(bytes: &[u8], key_start: usize) -> usize {
    let mut start = key_start;
    if start > 0 && matches!(bytes[start - 1], b'\'' | b'"') {
        start -= 1;
        while start > 0 && bytes[start - 1] == b'\\' {
            start -= 1;
        }
    }
    start
}

fn consume_optional_quote(bytes: &[u8], cursor: &mut usize) {
    let original = *cursor;
    while *cursor < bytes.len() && bytes[*cursor] == b'\\' {
        *cursor += 1;
    }
    if *cursor < bytes.len() && matches!(bytes[*cursor], b'\'' | b'"') {
        *cursor += 1;
    } else {
        *cursor = original;
    }
}

fn skip_whitespace(bytes: &[u8], cursor: &mut usize) {
    while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() {
        *cursor += 1;
    }
}

fn quoted_value_start(bytes: &[u8], cursor: usize) -> Option<(usize, usize)> {
    let mut quote_start = cursor;
    while quote_start < bytes.len() && bytes[quote_start] == b'\\' {
        quote_start += 1;
    }
    if quote_start < bytes.len() && matches!(bytes[quote_start], b'\'' | b'"') {
        Some((quote_start, quote_start - cursor))
    } else {
        None
    }
}

fn quoted_value_end(input: &str, quote_start: usize, quote_depth: usize) -> (usize, bool) {
    let bytes = input.as_bytes();
    let quote = bytes[quote_start];
    let mut cursor = quote_start + 1;
    while cursor < bytes.len() {
        if bytes[cursor] == quote {
            let mut slash_start = cursor;
            while slash_start > quote_start + 1 && bytes[slash_start - 1] == b'\\' {
                slash_start -= 1;
            }
            let slash_count = cursor - slash_start;
            if slash_count == quote_depth {
                if quote == b'\'' && bytes.get(cursor + 1) == Some(&quote) {
                    cursor += 2;
                    continue;
                }
                return (cursor + 1, true);
            }
        }
        cursor += 1;
    }
    (bytes.len(), false)
}

fn unquoted_value_end(bytes: &[u8], mut cursor: usize) -> usize {
    while cursor < bytes.len()
        && !matches!(
            bytes[cursor],
            b' ' | b'\t' | b'\r' | b'\n' | b',' | b'}' | b']' | b'\'' | b'"'
        )
    {
        cursor += 1;
    }
    cursor
}

fn yaml_block_scalar_end(
    input: &str,
    value_start: usize,
    assignment_start: usize,
) -> Option<(usize, bool)> {
    let bytes = input.as_bytes();
    let indicator = *bytes.get(value_start)?;
    if !matches!(indicator, b'|' | b'>') {
        return None;
    }

    let line_end = bytes[value_start..]
        .iter()
        .position(|byte| matches!(byte, b'\r' | b'\n'))
        .map_or(bytes.len(), |offset| value_start + offset);
    let mut cursor = value_start + 1;
    let mut has_chomping = false;
    let mut indentation = None;
    while cursor < line_end {
        match bytes[cursor] {
            b'+' | b'-' if !has_chomping => {
                has_chomping = true;
                cursor += 1;
            }
            b'1'..=b'9' if indentation.is_none() => {
                indentation = Some(usize::from(bytes[cursor] - b'0'));
                cursor += 1;
            }
            b' ' | b'\t' => break,
            _ => return None,
        }
    }

    let has_separator = cursor > value_start + 1 || cursor < line_end;
    while cursor < line_end && matches!(bytes[cursor], b' ' | b'\t') {
        cursor += 1;
    }
    if cursor < line_end && !(bytes[cursor] == b'#' && has_separator) {
        return None;
    }

    let assignment_line_start = line_start(bytes, assignment_start);
    let parent_indent = bytes[assignment_line_start..assignment_start]
        .iter()
        .take_while(|byte| matches!(**byte, b' ' | b'\t'))
        .count();
    let required_indent = indentation.map(|indent| parent_indent + indent);
    let Some(mut content_line) = line_after(bytes, line_end) else {
        return Some((bytes.len(), false));
    };
    let mut inferred_indent = required_indent;

    loop {
        let content_line_end = line_end_offset(bytes, content_line);
        let text_end = line_text_end(bytes, content_line, content_line_end);
        let line = &bytes[content_line..text_end];
        let line_indent = line.iter().take_while(|byte| **byte == b' ').count();
        let blank = line_indent == line.len();

        if !blank {
            let Some(required_indent) = inferred_indent else {
                if line_indent <= parent_indent {
                    return Some((line_break_start(bytes, content_line), true));
                }
                inferred_indent = Some(line_indent);
                let Some(next_line) = line_after(bytes, content_line_end) else {
                    return Some((bytes.len(), false));
                };
                content_line = next_line;
                if content_line == bytes.len() {
                    return Some((bytes.len(), false));
                }
                continue;
            };
            if line_indent < required_indent {
                return Some((line_break_start(bytes, content_line), true));
            }
        }

        let Some(next_line) = line_after(bytes, content_line_end) else {
            return Some((bytes.len(), false));
        };
        content_line = next_line;
        if content_line == bytes.len() {
            return Some((bytes.len(), false));
        }
    }
}

fn yaml_plain_scalar_end(
    input: &str,
    value_start: usize,
    assignment_start: usize,
    is_yaml_mapping: bool,
) -> Option<YamlPlainScalarEnd> {
    let bytes = input.as_bytes();
    if !is_yaml_mapping {
        return yaml_equals_scalar_end(input, value_start, assignment_start).map(
            |(end, closed)| YamlPlainScalarEnd {
                end,
                closed,
                split_at_comment: None,
            },
        );
    }

    let token_end = unquoted_value_end(bytes, value_start);
    if token_end == value_start {
        return None;
    }

    let line_end = line_end_offset(bytes, value_start);
    let assignment_line_start = line_start(bytes, assignment_start);
    let parent_indent = bytes[assignment_line_start..assignment_start]
        .iter()
        .take_while(|byte| matches!(**byte, b' ' | b'\t'))
        .count();
    let value_line_start = line_start(bytes, value_start);
    if value_line_start != assignment_line_start {
        let value_line_indent = bytes[value_line_start..value_start]
            .iter()
            .take_while(|byte| matches!(**byte, b' ' | b'\t'))
            .count();
        if value_line_indent <= parent_indent {
            let end = line_break_start(bytes, value_line_start);
            return Some(YamlPlainScalarEnd {
                end,
                closed: true,
                split_at_comment: None,
            });
        }
    }

    let first_line_end = line_text_end(bytes, value_start, line_end);
    let comment_start = yaml_plain_comment_start(bytes, value_start, first_line_end);
    let Some(mut continuation_line) = line_after(bytes, line_end) else {
        return Some(YamlPlainScalarEnd {
            end: comment_start.unwrap_or(first_line_end),
            closed: false,
            split_at_comment: None,
        });
    };

    if continuation_line == bytes.len() {
        return Some(YamlPlainScalarEnd {
            end: comment_start.unwrap_or(first_line_end),
            closed: false,
            split_at_comment: None,
        });
    }

    let continuation_start = continuation_line;
    let mut has_continuation = false;
    loop {
        let continuation_end = line_end_offset(bytes, continuation_line);
        let text_end = line_text_end(bytes, continuation_line, continuation_end);
        let line = &bytes[continuation_line..text_end];
        let line_indent = line
            .iter()
            .take_while(|byte| matches!(**byte, b' ' | b'\t'))
            .count();
        let blank = line_indent == line.len();

        if !blank && line_indent <= parent_indent {
            let scan_through = line_break_start(bytes, continuation_line);
            let split_at_comment = comment_start
                .filter(|_| has_continuation)
                .map(|comment| (comment, continuation_start));
            return Some(YamlPlainScalarEnd {
                end: if split_at_comment.is_some() {
                    scan_through
                } else {
                    comment_start.unwrap_or(scan_through)
                },
                closed: true,
                split_at_comment,
            });
        }
        if !blank {
            has_continuation = true;
        }

        let Some(next_line) = line_after(bytes, continuation_end) else {
            let split_at_comment = comment_start
                .filter(|_| has_continuation)
                .map(|comment| (comment, continuation_start));
            return Some(YamlPlainScalarEnd {
                end: if split_at_comment.is_some() {
                    bytes.len()
                } else {
                    comment_start.unwrap_or(bytes.len())
                },
                closed: false,
                split_at_comment,
            });
        };
        if next_line == bytes.len() {
            let split_at_comment = comment_start
                .filter(|_| has_continuation)
                .map(|comment| (comment, continuation_start));
            return Some(YamlPlainScalarEnd {
                end: if split_at_comment.is_some() {
                    bytes.len()
                } else {
                    comment_start.unwrap_or(bytes.len())
                },
                closed: false,
                split_at_comment,
            });
        }
        continuation_line = next_line;
    }
}

fn yaml_equals_scalar_end(
    input: &str,
    value_start: usize,
    assignment_start: usize,
) -> Option<(usize, bool)> {
    let bytes = input.as_bytes();
    let token_end = unquoted_value_end(bytes, value_start);
    if token_end == value_start {
        return None;
    }

    let line_end = line_end_offset(bytes, value_start);
    let mut after_token = token_end;
    while after_token < line_end && matches!(bytes[after_token], b' ' | b'\t') {
        after_token += 1;
    }
    if after_token < line_end {
        // Preserve the existing token boundary for `=` assignments.
        return None;
    }
    let mut continuation_line = line_after(bytes, line_end)?;
    if continuation_line == bytes.len() {
        return Some((bytes.len(), false));
    }

    let assignment_line_start = line_start(bytes, assignment_start);
    let parent_indent = bytes[assignment_line_start..assignment_start]
        .iter()
        .take_while(|byte| matches!(**byte, b' ' | b'\t'))
        .count();
    loop {
        let continuation_end = line_end_offset(bytes, continuation_line);
        let text_end = line_text_end(bytes, continuation_line, continuation_end);
        let line = &bytes[continuation_line..text_end];
        let line_indent = line
            .iter()
            .take_while(|byte| matches!(**byte, b' ' | b'\t'))
            .count();
        let blank = line_indent == line.len();

        if !blank && line_indent <= parent_indent {
            return Some((line_break_start(bytes, continuation_line), true));
        }

        let Some(next_line) = line_after(bytes, continuation_end) else {
            return Some((bytes.len(), false));
        };
        if next_line == bytes.len() {
            return Some((bytes.len(), false));
        }
        continuation_line = next_line;
    }
}

fn yaml_plain_comment_start(bytes: &[u8], value_start: usize, line_end: usize) -> Option<usize> {
    bytes[value_start..line_end]
        .windows(2)
        .position(|pair| matches!(pair[0], b' ' | b'\t') && pair[1] == b'#')
        .map(|offset| trim_ascii_whitespace_end(bytes, value_start, value_start + offset))
}

fn authorization_header_value_end(input: &str, value_start: usize) -> (usize, bool) {
    let bytes = input.as_bytes();
    let mut line_end = value_start;
    while line_end < bytes.len()
        && !matches!(bytes[line_end], b',' | b';' | b'\r' | b'\n' | b'}' | b']')
    {
        line_end += 1;
    }

    if line_end < bytes.len() && matches!(bytes[line_end], b'\r' | b'\n') {
        let header_end = trim_ascii_whitespace_end(bytes, value_start, line_end);
        let Some(mut continuation_line) = line_after(bytes, line_end) else {
            return (bytes.len(), false);
        };
        if continuation_line == bytes.len() {
            return (bytes.len(), false);
        }
        if !matches!(bytes[continuation_line], b' ' | b'\t') {
            return (header_end, true);
        }

        loop {
            let continuation_end = line_end_offset(bytes, continuation_line);
            let text_end = line_text_end(bytes, continuation_line, continuation_end);
            if text_end == continuation_line {
                return (line_break_start(bytes, continuation_line), true);
            }
            if !matches!(bytes[continuation_line], b' ' | b'\t') {
                return (line_break_start(bytes, continuation_line), true);
            }
            let Some(next_line) = line_after(bytes, continuation_end) else {
                return (bytes.len(), false);
            };
            if next_line == bytes.len() {
                return (bytes.len(), false);
            }
            if !matches!(bytes[next_line], b' ' | b'\t') {
                return (line_break_start(bytes, next_line), true);
            }
            continuation_line = next_line;
        }
    }

    (
        trim_ascii_whitespace_end(bytes, value_start, line_end),
        true,
    )
}

fn line_start(bytes: &[u8], offset: usize) -> usize {
    bytes[..offset]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |newline| newline + 1)
}

fn line_end_offset(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|byte| matches!(byte, b'\r' | b'\n'))
        .map_or(bytes.len(), |offset| start + offset)
}

fn line_text_end(bytes: &[u8], start: usize, end: usize) -> usize {
    if end > start && bytes[end - 1] == b'\r' {
        end - 1
    } else {
        end
    }
}

fn line_after(bytes: &[u8], end: usize) -> Option<usize> {
    match bytes.get(end) {
        Some(b'\r') if bytes.get(end + 1) == Some(&b'\n') => Some(end + 2),
        Some(b'\n') => Some(end + 1),
        Some(b'\r') => Some(end + 1),
        _ => None,
    }
}

fn line_break_start(bytes: &[u8], next_line_start: usize) -> usize {
    if next_line_start >= 2 && bytes[next_line_start - 2..next_line_start] == *b"\r\n" {
        next_line_start - 2
    } else if next_line_start > 0 {
        next_line_start - 1
    } else {
        0
    }
}

fn trim_ascii_whitespace_end(bytes: &[u8], start: usize, mut end: usize) -> usize {
    while end > start && matches!(bytes[end - 1], b' ' | b'\t' | b'\r') {
        end -= 1;
    }
    end
}

fn private_key_value_end(input: &str, value_start: usize) -> Option<(usize, bool)> {
    let input_tail = input.get(value_start..)?;
    let span = private_key_spans(input_tail)?
        .into_iter()
        .find(|span| span.start == 0)?;
    Some((value_start + span.end, span.closed))
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

fn private_key_matcher() -> Option<&'static Regex> {
    static PRIVATE_KEY: OnceLock<Option<Regex>> = OnceLock::new();
    PRIVATE_KEY
        .get_or_init(|| {
            Regex::new(
                r"(?is)(-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----).*?(-----END [A-Z0-9 ]*PRIVATE KEY-----|\z)",
            )
            .ok()
        })
        .as_ref()
}

/// Streaming redactor from the build-split evidence implementation.
/// Whole-text redaction keeps the newer account-consolidation span analyzer;
/// this child module preserves the branch's stateful BuildKit/PEM stream logic.
pub use stream_redactor::StreamRedactor;

mod stream_redactor {

    use std::borrow::Cow;
    use std::collections::HashMap;
    use std::sync::OnceLock;

    use regex::Regex;

    const REDACTED: &str = "<redacted>";
    pub(crate) const MAX_STREAM_LINE_BYTES: usize = 64 * 1024;
    pub(crate) const MAX_STRUCTURED_DEPTH: usize = 256;
    pub(crate) const MAX_ACTIVE_ENVELOPES: usize = 64;
    pub(crate) const MAX_PEM_LABEL_BYTES: usize = 128;

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
        Pem { label: String },
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

    #[derive(Clone, Copy)]
    struct PemMarker<'a> {
        start: usize,
        end: usize,
        label: &'a str,
    }

    enum PemScan {
        Complete(usize),
        Incomplete(String),
        Malformed,
    }

    enum PemContinuation {
        Complete(usize),
        Open,
        Malformed,
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
                StreamMode::Pem { label } => match scan_pem_continuation(line, &label) {
                    PemContinuation::Complete(end) => {
                        self.process_suffix(&line[end..], line_prefix, mode)
                    }
                    PemContinuation::Open => {
                        *mode = StreamMode::Pem { label };
                        Vec::new()
                    }
                    PemContinuation::Malformed => {
                        *mode = StreamMode::FailedClosed;
                        Vec::new()
                    }
                },
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
                    (Some(begin), Some(assignment)) => begin.start < assignment.start,
                    (Some(_), None) => true,
                    _ => false,
                };
                if pem_precedes_assignment {
                    let begin = pem_begin.expect("PEM marker precedes any assignment");
                    output.push_str(&redact_unstructured_text(&remaining[..begin.start]));
                    output.push_str(REDACTED);
                    match scan_pem_block(&remaining[begin.start..]) {
                        PemScan::Complete(end) => {
                            cursor += begin.start + end;
                            continue;
                        }
                        PemScan::Incomplete(label) => {
                            *mode = StreamMode::Pem { label };
                            return vec![output];
                        }
                        PemScan::Malformed => {
                            *mode = StreamMode::FailedClosed;
                            return vec![output];
                        }
                    }
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
                    if let Some(end) =
                        quoted_value_end(bytes, value_start + delimiter_len, &mut quoted)
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
                    let begin = value_start + begin.start;
                    output.push_str(REDACTED);
                    match scan_pem_block(&remaining[begin..]) {
                        PemScan::Complete(end) => {
                            cursor += begin + end;
                            continue;
                        }
                        PemScan::Incomplete(label) => {
                            *mode = StreamMode::Pem { label };
                            return vec![output];
                        }
                        PemScan::Malformed => {
                            *mode = StreamMode::FailedClosed;
                            return vec![output];
                        }
                    }
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
                    || (byte == b'#'
                        && offset > 0
                        && bytes[start + offset - 1].is_ascii_whitespace())
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

    fn private_key_begin(input: &str) -> Option<PemMarker<'_>> {
        let captures = private_key_begin_matcher().captures(input)?;
        let marker = captures.get(0)?;
        let label = captures.get(1)?;
        Some(PemMarker {
            start: marker.start(),
            end: marker.end(),
            label: label.as_str(),
        })
    }

    fn private_key_footer(input: &str) -> Option<PemMarker<'_>> {
        let captures = private_key_footer_matcher().captures(input)?;
        let marker = captures.get(0)?;
        let label = captures.get(1)?;
        Some(PemMarker {
            start: marker.start(),
            end: marker.end(),
            label: label.as_str(),
        })
    }

    fn scan_pem_block(input: &str) -> PemScan {
        let Some(begin) = private_key_begin(input) else {
            return PemScan::Malformed;
        };
        if begin.label.len() > MAX_PEM_LABEL_BYTES {
            return PemScan::Malformed;
        }

        let body = &input[begin.end..];
        let nested_begin = private_key_begin(body);
        let footer = private_key_footer(body);
        let body_end = footer.map_or(body.len(), |footer| footer.start);
        if body.as_bytes()[..body_end]
            .iter()
            .any(|byte| matches!(byte, b'\'' | b'"'))
        {
            return PemScan::Malformed;
        }
        let nested_precedes_footer = nested_begin.is_some_and(|nested_begin| {
            footer.is_none_or(|footer| nested_begin.start < footer.start)
        });
        if nested_precedes_footer {
            return PemScan::Malformed;
        }
        let Some(footer) = footer else {
            return PemScan::Incomplete(begin.label.to_owned());
        };
        if footer.label != begin.label {
            return PemScan::Malformed;
        }
        PemScan::Complete(begin.end + footer.end)
    }

    fn scan_pem_continuation(input: &str, expected_label: &str) -> PemContinuation {
        let nested_begin = private_key_begin(input);
        let footer = private_key_footer(input);
        let body_end = footer.map_or(input.len(), |footer| footer.start);
        if input.as_bytes()[..body_end]
            .iter()
            .any(|byte| matches!(byte, b'\'' | b'"'))
        {
            return PemContinuation::Malformed;
        }
        let nested_precedes_footer = nested_begin.is_some_and(|nested_begin| {
            footer.is_none_or(|footer| nested_begin.start < footer.start)
        });
        if nested_precedes_footer {
            return PemContinuation::Malformed;
        }
        let Some(footer) = footer else {
            return PemContinuation::Open;
        };
        if footer.label != expected_label {
            return PemContinuation::Malformed;
        }
        PemContinuation::Complete(footer.end)
    }

    fn private_key_begin_matcher() -> &'static Regex {
        static MATCHER: OnceLock<Regex> = OnceLock::new();
        MATCHER.get_or_init(|| {
            Regex::new(r"-----BEGIN ([A-Z0-9 ]*PRIVATE KEY)-----")
                .expect("valid private key begin matcher")
        })
    }

    fn private_key_footer_matcher() -> &'static Regex {
        static MATCHER: OnceLock<Regex> = OnceLock::new();
        MATCHER.get_or_init(|| {
            Regex::new(r"-----END ([A-Z0-9 ]*PRIVATE KEY)-----")
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
}

#[cfg(test)]
mod tests;
