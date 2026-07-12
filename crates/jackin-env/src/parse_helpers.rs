//! Parse helpers shared across operator-env modules.

/// Extract a host env var name from `$NAME` or `${NAME}` forms.
///
/// Returns `None` when the string is not a host ref or the name is invalid.
pub fn parse_host_ref(value: &str) -> Option<&str> {
    if let Some(rest) = value.strip_prefix("${")
        && let Some(name) = rest.strip_suffix('}')
        && is_valid_env_name(name)
    {
        return Some(name);
    }
    if let Some(name) = value.strip_prefix('$')
        && !name.is_empty()
        && is_valid_env_name(name)
    {
        return Some(name);
    }
    None
}

pub(crate) fn is_valid_env_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
