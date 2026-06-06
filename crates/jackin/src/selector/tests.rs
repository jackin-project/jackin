//! Tests for `selector`.
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
        RoleSelector::parse("jk-agent-smith"),
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
    let selector = Selector::parse("jk-k7p9m2xq-chainargos-thearchitect").unwrap();
    assert_eq!(
        selector,
        Selector::Container("jk-k7p9m2xq-chainargos-thearchitect".to_owned())
    );
}

#[test]
fn parses_container_selector_no_workspace() {
    let selector = Selector::parse("jk-k7p9m2xq-agentsmith").unwrap();
    assert_eq!(
        selector,
        Selector::Container("jk-k7p9m2xq-agentsmith".to_owned())
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
