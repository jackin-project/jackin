use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    Role(RoleSelector),
    Container(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleSelector {
    pub namespace: Option<String>,
    pub name: String,
}

impl fmt::Display for RoleSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(namespace) = &self.namespace {
            write!(f, "{namespace}/{}", self.name)
        } else {
            f.write_str(&self.name)
        }
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SelectorError {
    #[error("selector cannot be empty")]
    Empty,
    #[error("invalid selector: {0}")]
    Invalid(String),
}

impl RoleSelector {
    pub fn new(namespace: Option<&str>, name: &str) -> Self {
        Self {
            namespace: namespace.map(ToString::to_string),
            name: name.to_string(),
        }
    }

    /// Parse a role selector. Input is lowercased before validation so
    /// `ChainArgos/Agent-Brown` and `chainargos/agent-brown` both produce
    /// the same `RoleSelector`. This matches GitHub's case-insensitive
    /// org/user routing and the Docker constraint that container/image
    /// names must be lowercase. Display names live in the manifest's
    /// `[identity].name` field, so case preservation has its own slot.
    pub fn parse(input: &str) -> Result<Self, SelectorError> {
        if input.is_empty() {
            return Err(SelectorError::Empty);
        }

        let normalized = input.to_ascii_lowercase();
        let input = normalized.as_str();

        if !input.contains('/') {
            return (is_valid_role_segment(input) && !is_reserved_builtin_role_name(input))
                .then(|| Self::new(None, input))
                .ok_or_else(|| SelectorError::Invalid(input.to_string()));
        }

        let mut parts = input.split('/');
        if let (Some(namespace), Some(name), None) = (parts.next(), parts.next(), parts.next())
            && is_valid_role_segment(namespace)
            && is_valid_role_segment(name)
        {
            return Ok(Self::new(Some(namespace), name));
        }

        Err(SelectorError::Invalid(input.to_string()))
    }

    pub fn key(&self) -> String {
        self.to_string()
    }
}

impl TryFrom<&str> for RoleSelector {
    type Error = SelectorError;

    /// Idiomatic wrapper around [`RoleSelector::parse`]. Exists so callers
    /// that rely on `TryFrom` conversion traits (including generic code and
    /// `try_into()` call sites) can convert a `&str` without having to
    /// reach for the inherent `parse` method.
    fn try_from(input: &str) -> Result<Self, Self::Error> {
        Self::parse(input)
    }
}

impl Selector {
    pub fn parse(input: &str) -> Result<Self, SelectorError> {
        if input.is_empty() {
            return Err(SelectorError::Empty);
        }

        if is_valid_container_name(input) {
            return Ok(Self::Container(input.to_string()));
        }

        if !input.contains('/')
            && let Some((base, suffix)) = input.rsplit_once("-clone-")
            && is_valid_role_segment(base)
            && suffix.chars().all(|ch| ch.is_ascii_digit())
        {
            return Ok(Self::Container(format!("jackin-{input}")));
        }

        Ok(Self::Role(RoleSelector::parse(input)?))
    }
}

impl TryFrom<&str> for Selector {
    type Error = SelectorError;

    /// Idiomatic wrapper around [`Selector::parse`]. See the analogous impl
    /// on [`RoleSelector`] for rationale.
    fn try_from(input: &str) -> Result<Self, Self::Error> {
        Self::parse(input)
    }
}

fn is_valid_role_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn is_valid_container_name(value: &str) -> bool {
    value.strip_prefix("jackin-").is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    })
}

fn is_reserved_builtin_role_name(value: &str) -> bool {
    value.starts_with("jackin-")
        || value.rsplit_once("-clone-").is_some_and(|(base, suffix)| {
            is_valid_role_segment(base) && suffix.chars().all(|ch| ch.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builtin_class_selector() {
        let selector = Selector::parse("agent-smith").unwrap();
        assert_eq!(
            selector,
            Selector::Role(RoleSelector::new(None, "agent-smith"))
        );
    }

    #[test]
    fn class_parser_rejects_reserved_builtin_names() {
        assert!(matches!(
            RoleSelector::parse("jackin-agent-smith"),
            Err(SelectorError::Invalid(_))
        ));
        assert!(matches!(
            RoleSelector::parse("agent-smith-clone-1"),
            Err(SelectorError::Invalid(_))
        ));
    }

    #[test]
    fn parses_namespaced_class_selector() {
        let selector = Selector::parse("chainargos/the-architect").unwrap();
        assert_eq!(
            selector,
            Selector::Role(RoleSelector::new(Some("chainargos"), "the-architect"))
        );
    }

    #[test]
    fn parses_container_selector() {
        let selector = Selector::parse("jackin-chainargos-the-architect-clone-1").unwrap();
        assert_eq!(
            selector,
            Selector::Container("jackin-chainargos-the-architect-clone-1".to_string())
        );
    }

    #[test]
    fn parses_container_selector_with_namespace_separator() {
        let selector = Selector::parse("jackin-chainargos__the-architect").unwrap();
        assert_eq!(
            selector,
            Selector::Container("jackin-chainargos__the-architect".to_string())
        );
    }

    #[test]
    fn parses_clone_shorthand_selector() {
        let selector = Selector::parse("agent-smith-clone-1").unwrap();
        assert_eq!(
            selector,
            Selector::Container("jackin-agent-smith-clone-1".to_string())
        );
    }

    #[test]
    fn rejects_malformed_namespaced_selector() {
        assert!(matches!(
            Selector::parse("foo/bar/baz"),
            Err(SelectorError::Invalid(_))
        ));
        assert!(matches!(
            Selector::parse("foo/../bar"),
            Err(SelectorError::Invalid(_))
        ));
    }

    #[test]
    fn parse_normalizes_uppercase_to_lowercase() {
        // Bare role names: uppercase tolerated, lowercased on parse.
        assert_eq!(
            Selector::parse("Agent-Smith").unwrap(),
            Selector::Role(RoleSelector::new(None, "agent-smith"))
        );

        // Namespaced (GitHub-style): both segments lowercased so
        // `ChainArgos/Agent-Brown` and `chainargos/agent-brown` dedupe.
        assert_eq!(
            Selector::parse("ChainArgos/Agent-Brown").unwrap(),
            Selector::Role(RoleSelector::new(Some("chainargos"), "agent-brown"))
        );
    }
}
