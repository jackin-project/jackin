use crate::selector::ClassSelector;

pub fn runtime_slug(selector: &ClassSelector) -> String {
    selector.namespace.as_ref().map_or_else(
        || selector.name.clone(),
        |namespace| format!("{namespace}__{}", selector.name),
    )
}

pub fn primary_container_name(selector: &ClassSelector) -> String {
    format!("jackin-{}", runtime_slug(selector))
}

pub fn next_container_name(selector: &ClassSelector, existing: &[String]) -> String {
    let primary = primary_container_name(selector);
    if !existing.iter().any(|name| name == &primary) {
        return primary;
    }

    let mut clone_index = 1;
    loop {
        let candidate = format!("{primary}-clone-{clone_index}");
        if !existing.iter().any(|name| name == &candidate) {
            return candidate;
        }
        clone_index += 1;
    }
}

pub fn class_family_matches(selector: &ClassSelector, container_name: &str) -> bool {
    let primary = primary_container_name(selector);
    container_name == primary || container_name.starts_with(&format!("{primary}-clone-"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selector::ClassSelector;

    #[test]
    fn picks_next_clone_name() {
        let selector = ClassSelector::new(None, "agent-smith");
        let existing = vec![
            "jackin-agent-smith".to_string(),
            "jackin-agent-smith-clone-1".to_string(),
        ];

        let name = next_container_name(&selector, &existing);

        assert_eq!(name, "jackin-agent-smith-clone-2");
    }

    #[test]
    fn distinguishes_namespaced_and_flat_class_container_names() {
        let namespaced = ClassSelector::new(Some("chainargos"), "the-architect");
        let flat = ClassSelector::new(None, "chainargos-the-architect");

        assert_ne!(
            primary_container_name(&namespaced),
            primary_container_name(&flat)
        );
    }
}
