use crate::selector::RoleSelector;

pub fn runtime_slug(selector: &RoleSelector) -> String {
    selector.namespace.as_ref().map_or_else(
        || selector.name.clone(),
        |namespace| format!("{namespace}__{}", selector.name),
    )
}

pub fn primary_container_name(selector: &RoleSelector) -> String {
    format!("jackin-{}", runtime_slug(selector))
}

pub fn next_container_name(selector: &RoleSelector, existing: &[String]) -> String {
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

pub fn class_family_matches(selector: &RoleSelector, container_name: &str) -> bool {
    let primary = primary_container_name(selector);
    container_name == primary || container_name.starts_with(&format!("{primary}-clone-"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selector::RoleSelector;

    #[test]
    fn picks_next_clone_name() {
        let selector = RoleSelector::new(None, "agent-smith");
        let existing = vec![
            "jackin-agent-smith".to_string(),
            "jackin-agent-smith-clone-1".to_string(),
        ];

        let name = next_container_name(&selector, &existing);

        assert_eq!(name, "jackin-agent-smith-clone-2");
    }

    #[test]
    fn distinguishes_namespaced_and_flat_class_container_names() {
        let namespaced = RoleSelector::new(Some("chainargos"), "the-architect");
        let flat = RoleSelector::new(None, "chainargos-the-architect");

        assert_ne!(
            primary_container_name(&namespaced),
            primary_container_name(&flat)
        );
    }
}
