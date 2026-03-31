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
        if input.starts_with("agent-") {
            return Ok(Self::Container(input.to_string()));
        }
        if let Some((base, suffix)) = input.rsplit_once("-clone-") {
            if !base.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()) {
                return Ok(Self::Container(format!("agent-{input}")));
            }
        }
        if let Some((namespace, name)) = input.split_once('/') {
            if !namespace.is_empty() && !name.is_empty() {
                return Ok(Self::Class(ClassSelector::new(Some(namespace), name)));
            }
        }
        if input.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-') {
            return Ok(Self::Class(ClassSelector::new(None, input)));
        }
        Err(SelectorError::Invalid(input.to_string()))
    }
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
}
