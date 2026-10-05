// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Parse and format `op://vault/item/field` references used by the 1Password
//! secret-resolution paths.
//!
//! Vocabulary shared by `jackin-env` (token resolution) and `jackin-console`
//! (the Auth-tab picker); it lives here so neither has to reach into the other
//! for the grammar. Not responsible for calling the `op` CLI or rendering any
//! widget.

/// Structured parts of an `op://...` reference.
///
/// Syntax: `op://<vault>/<item>/[<section>/]<field>`. Account scope is
/// not encoded in the path; multi-account picks live separately on the
/// selected account. See
/// <https://developer.1password.com/docs/cli/secret-reference-syntax/>.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpReferenceParts {
    /// Vault name or ID segment.
    pub vault: String,
    /// Item name or ID segment.
    pub item: String,
    /// Optional section between item and field.
    pub section: Option<String>,
    /// Field name or ID segment.
    pub field: String,
}

/// Human-readable parts of a local 1Password breadcrumb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpBreadcrumbParts {
    /// Vault label.
    pub vault: String,
    /// Item label.
    pub item: String,
    /// Optional item subtitle used to distinguish same-name items.
    pub item_subtitle: Option<String>,
    /// Optional section label.
    pub section: Option<String>,
    /// Field label.
    pub field: String,
    /// Optional query suffix carried by older snapshots.
    pub attribute_query: Option<String>,
}

impl OpReferenceParts {
    /// Operator-facing copy-pasteable `op item delete` invocation.
    pub fn manual_delete_hint(&self) -> impl std::fmt::Display + '_ {
        struct Hint<'a> {
            item: &'a str,
            vault: &'a str,
        }
        impl std::fmt::Display for Hint<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "op item delete {} --vault {}", self.item, self.vault)
            }
        }
        Hint {
            item: &self.item,
            vault: &self.vault,
        }
    }
}

/// Whether an identifier is a non-empty RFC 3986 URI path component.
///
/// Canonical references use opaque 1Password IDs, not display labels. Raw
/// separators, query/fragment delimiters, whitespace, and control characters
/// are rejected rather than rewritten; valid percent triplets remain literal.
#[must_use]
pub fn is_valid_op_reference_path_component(component: &str) -> bool {
    let bytes = component.as_bytes();
    if bytes.is_empty() {
        return false;
    }

    let mut index = 0;
    while index < bytes.len() {
        let Some(&byte) = bytes.get(index) else {
            return false;
        };
        let allowed = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'.'
                    | b'_'
                    | b'~'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
                    | b':'
                    | b'@'
            );
        if allowed {
            index += 1;
            continue;
        }
        if byte == b'%' {
            let Some(&high) = bytes.get(index.saturating_add(1)) else {
                return false;
            };
            let Some(&low) = bytes.get(index.saturating_add(2)) else {
                return false;
            };
            if is_hex_digit(high) && is_hex_digit(low) {
                index = index.saturating_add(3);
                continue;
            }
        }
        return false;
    }
    true
}

/// Format a reference from known vault/item/section/field IDs.
///
/// `None` means at least one ID cannot occupy exactly one URI path segment.
/// Omitting `section_id` deliberately keeps the three-segment reference.
#[must_use]
pub fn build_op_reference(
    vault_id: &str,
    item_id: &str,
    section_id: Option<&str>,
    field_id: &str,
) -> Option<String> {
    if !is_valid_op_reference_path_component(vault_id)
        || !is_valid_op_reference_path_component(item_id)
        || !is_valid_op_reference_path_component(field_id)
        || section_id.is_some_and(|id| !is_valid_op_reference_path_component(id))
    {
        return None;
    }

    Some(match section_id {
        Some(section_id) => format!("op://{vault_id}/{item_id}/{section_id}/{field_id}"),
        None => format!("op://{vault_id}/{item_id}/{field_id}"),
    })
}

fn is_hex_digit(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

/// Escape a component in the local `OpRef.path` breadcrumb.
///
/// This display-path encoding is separate from 1Password URI syntax. It keeps
/// the breadcrumb's `/`, `?`, and item-subtitle brackets unambiguous while
/// preserving the original label for the console parser to display.
#[must_use]
pub fn encode_op_breadcrumb_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for ch in segment.chars() {
        if matches!(ch, '%' | '/' | '?' | '[' | ']') {
            let byte = ch as u8;
            encoded.push('%');
            encoded.push(hex_digit(byte >> 4));
            encoded.push(hex_digit(byte & 0x0f));
        } else {
            encoded.push(ch);
        }
    }
    encoded
}

/// Decode a component escaped by [`encode_op_breadcrumb_segment`].
#[must_use]
pub fn decode_op_breadcrumb_segment(segment: &str) -> Option<String> {
    let mut decoded = String::with_capacity(segment.len());
    let mut chars = segment.chars();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let hi = hex_value(chars.next()?)?;
            let lo = hex_value(chars.next()?)?;
            let byte = (hi << 4) | lo;
            let decoded_char = char::from_u32(u32::from(byte))?;
            if !matches!(decoded_char, '%' | '/' | '?' | '[' | ']') {
                return None;
            }
            decoded.push(decoded_char);
        } else {
            decoded.push(ch);
        }
    }
    Some(decoded)
}

/// Parse a v1 percent-escaped local `OpRef.path` breadcrumb.
#[must_use]
pub fn parse_op_breadcrumb_path(path: &str) -> Option<OpBreadcrumbParts> {
    if path.is_empty() {
        return None;
    }
    let (path_no_query, attribute_query) =
        path.split_once('?').map_or((path, None), |(path, query)| {
            (path, Some(format!("?{query}")))
        });
    let segments: Vec<&str> = path_no_query.split('/').collect();
    if segments.iter().any(|segment| segment.is_empty()) {
        return None;
    }
    let (vault, item_segment, section, field) = match segments.as_slice() {
        [vault, item, field] => (*vault, *item, None, *field),
        [vault, item, section, field] => (*vault, *item, Some(*section), *field),
        _ => return None,
    };
    let (item_encoded, subtitle_encoded) = split_bracket_subtitle(item_segment);
    Some(OpBreadcrumbParts {
        vault: decode_op_breadcrumb_segment(vault)?,
        item: decode_op_breadcrumb_segment(item_encoded)?,
        item_subtitle: match subtitle_encoded {
            Some(subtitle) => Some(decode_op_breadcrumb_segment(subtitle)?),
            None => None,
        },
        section: match section {
            Some(section) => Some(decode_op_breadcrumb_segment(section)?),
            None => None,
        },
        field: decode_op_breadcrumb_segment(field)?,
        attribute_query,
    })
}

/// Render a v1 local breadcrumb as plain text for CLI, preview, and error output.
#[must_use]
pub fn display_op_breadcrumb_path(path: &str) -> String {
    let Some(parts) = parse_op_breadcrumb_path(path) else {
        return path.to_owned();
    };
    let item = match parts.item_subtitle {
        Some(subtitle) => format!("{}[{}]", parts.item, subtitle),
        None => parts.item,
    };
    let mut display = format!("{}/{}", parts.vault, item);
    if let Some(section) = parts.section {
        display.push('/');
        display.push_str(&section);
    }
    display.push('/');
    display.push_str(&parts.field);
    if let Some(query) = parts.attribute_query {
        display.push_str(&query);
    }
    display
}

/// Convert an unversioned config breadcrumb into the v1 escaped value.
///
/// The old wire schema stored display text literally, so percent triplets and
/// question marks in component labels are literal characters. This function
/// never decodes them. A value is migrated only when its old 3/4-segment
/// structure is recoverable. An old breadcrumb may omit the URI query; when a
/// query suffix is present, it must exactly match the stored `op://` URI.
#[must_use]
pub fn encode_legacy_op_breadcrumb(path: &str, reference: &str) -> Option<String> {
    let query = reference.split_once('?').map(|(_, query)| query);
    let reference = parse_op_reference(reference)?;
    let (path_no_query, had_query_suffix) = match query {
        Some(query) => match path.strip_suffix(&format!("?{query}")) {
            Some(path) => (path, true),
            // Legacy producers keep the query in `op`, not in the display
            // breadcrumb. Accept that representation while still rejecting
            // an unmatched `?` that could be inconsistent query data.
            None if !path.contains('?') => (path, false),
            None => return None,
        },
        None => (path, false),
    };
    let segments: Vec<&str> = path_no_query.split('/').collect();
    let expected_segment_count = if reference.section.is_some() { 4 } else { 3 };
    if segments.len() != expected_segment_count || segments.iter().any(|segment| segment.is_empty())
    {
        return None;
    }
    let encoded = match segments.as_slice() {
        [vault, item, field] => format!(
            "{}/{}/{}",
            encode_op_breadcrumb_segment(vault),
            encode_legacy_item_segment(item),
            encode_op_breadcrumb_segment(field)
        ),
        [vault, item, section, field] => format!(
            "{}/{}/{}/{}",
            encode_op_breadcrumb_segment(vault),
            encode_legacy_item_segment(item),
            encode_op_breadcrumb_segment(section),
            encode_op_breadcrumb_segment(field)
        ),
        _ => return None,
    };
    Some(match (query, had_query_suffix) {
        (Some(query), true) => format!("{encoded}?{query}"),
        _ => encoded,
    })
}

fn encode_legacy_item_segment(segment: &str) -> String {
    if let Some(open) = segment.rfind('[')
        && segment.ends_with(']')
        && open < segment.len() - 1
    {
        let Some(item) = segment.get(..open) else {
            return encode_op_breadcrumb_segment(segment);
        };
        let Some(subtitle) = segment
            .get(open..)
            .and_then(|suffix| suffix.strip_prefix('['))
            .and_then(|suffix| suffix.strip_suffix(']'))
        else {
            return encode_op_breadcrumb_segment(segment);
        };
        return format!(
            "{}[{}]",
            encode_op_breadcrumb_segment(item),
            encode_op_breadcrumb_segment(subtitle)
        );
    }
    encode_op_breadcrumb_segment(segment)
}

fn split_bracket_subtitle(segment: &str) -> (&str, Option<&str>) {
    if let Some(open) = segment.rfind('[')
        && segment.ends_with(']')
        && open < segment.len() - 1
    {
        let Some(item) = segment.get(..open) else {
            return (segment, None);
        };
        let Some(subtitle) = segment
            .get(open..)
            .and_then(|suffix| suffix.strip_prefix('['))
            .and_then(|suffix| suffix.strip_suffix(']'))
        else {
            return (segment, None);
        };
        return (item, Some(subtitle));
    }
    (segment, None)
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => char::from(b'0' + nibble),
        10..=15 => char::from(b'A' + nibble - 10),
        _ => '0',
    }
}

fn hex_value(ch: char) -> Option<u8> {
    match ch {
        '0'..='9' => Some(ch as u8 - b'0'),
        'a'..='f' => Some(ch as u8 - b'a' + 10),
        'A'..='F' => Some(ch as u8 - b'A' + 10),
        _ => None,
    }
}

/// Parse an `op://vault/item/field` (or sectioned) secret reference.
#[must_use]
pub fn parse_op_reference(value: &str) -> Option<OpReferenceParts> {
    let path = value.strip_prefix("op://")?;
    let path = path.split('?').next().unwrap_or(path);
    let parts: Vec<&str> = path.split('/').collect();
    if parts.iter().any(|s| s.is_empty()) {
        return None;
    }
    match parts.as_slice() {
        [vault, item, field] => Some(OpReferenceParts {
            vault: (*vault).to_owned(),
            item: (*item).to_owned(),
            section: None,
            field: (*field).to_owned(),
        }),
        [vault, item, section, field] => Some(OpReferenceParts {
            vault: (*vault).to_owned(),
            item: (*item).to_owned(),
            section: Some((*section).to_owned()),
            field: (*field).to_owned(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
