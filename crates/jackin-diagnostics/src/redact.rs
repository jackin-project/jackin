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

#[cfg(test)]
mod tests;
