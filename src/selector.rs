use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    Class(ClassSelector),
    Container(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassSelector {
    pub namespace: Option<String>,
    pub name: String,
}

#[derive(Debug, Error)]
pub enum SelectorError {
    #[error("selector cannot be empty")]
    Empty,
    #[error("invalid selector: {0}")]
    Invalid(String),
}

impl ClassSelector {
    pub fn new(namespace: Option<&str>, name: &str) -> Self {
        Self {
            namespace: namespace.map(|value| value.to_string()),
            name: name.to_string(),
        }
    }

    pub fn parse(input: &str) -> Result<Self, SelectorError> {
        if input.is_empty() {
            return Err(SelectorError::Empty);
        }

        if !input.contains('/') {
            return (is_valid_class_segment(input) && !is_reserved_builtin_class_name(input))
                .then(|| Self::new(None, input))
                .ok_or_else(|| SelectorError::Invalid(input.to_string()));
        }

        let mut parts = input.split('/');
        if let (Some(namespace), Some(name), None) = (parts.next(), parts.next(), parts.next()) {
            if is_valid_class_segment(namespace) && is_valid_class_segment(name) {
                return Ok(Self::new(Some(namespace), name));
            }
        }

        Err(SelectorError::Invalid(input.to_string()))
    }

    pub fn key(&self) -> String {
        match &self.namespace {
            Some(namespace) => format!("{namespace}/{}", self.name),
            None => self.name.clone(),
        }
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

        if !input.contains('/') {
            if let Some((base, suffix)) = input.rsplit_once("-clone-") {
                if is_valid_class_segment(base) && suffix.chars().all(|ch| ch.is_ascii_digit()) {
                    return Ok(Self::Container(format!("agent-{input}")));
                }
            }
        }

        Ok(Self::Class(ClassSelector::parse(input)?))
    }
}

fn is_valid_class_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn is_valid_container_name(value: &str) -> bool {
    value.starts_with("agent-") && is_valid_class_segment(&value["agent-".len()..])
}

fn is_reserved_builtin_class_name(value: &str) -> bool {
    value.starts_with("agent-")
        || value
            .rsplit_once("-clone-")
            .is_some_and(|(base, suffix)| is_valid_class_segment(base) && suffix.chars().all(|ch| ch.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builtin_class_selector() {
        let selector = Selector::parse("smith").unwrap();
        assert_eq!(selector, Selector::Class(ClassSelector::new(None, "smith")));
    }

    #[test]
    fn class_parser_rejects_reserved_builtin_names() {
        assert!(matches!(
            ClassSelector::parse("agent-smith"),
            Err(SelectorError::Invalid(_))
        ));
        assert!(matches!(
            ClassSelector::parse("smith-clone-1"),
            Err(SelectorError::Invalid(_))
        ));
    }

    #[test]
    fn parses_namespaced_class_selector() {
        let selector = Selector::parse("chainargos/smith").unwrap();
        assert_eq!(
            selector,
            Selector::Class(ClassSelector::new(Some("chainargos"), "smith"))
        );
    }

    #[test]
    fn parses_container_selector() {
        let selector = Selector::parse("agent-chainargos-smith-clone-1").unwrap();
        assert_eq!(
            selector,
            Selector::Container("agent-chainargos-smith-clone-1".to_string())
        );
    }

    #[test]
    fn parses_clone_shorthand_selector() {
        let selector = Selector::parse("smith-clone-1").unwrap();
        assert_eq!(selector, Selector::Container("agent-smith-clone-1".to_string()));
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
        assert!(matches!(
            Selector::parse("Foo/bar"),
            Err(SelectorError::Invalid(_))
        ));
    }
}
