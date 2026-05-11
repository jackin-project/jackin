use crate::selector::RoleSelector;
use sha2::{Digest, Sha256};

const CONTAINER_PREFIX: &str = "jackin";
const INSTANCE_ID_LEN: usize = 8;
const ROLE_BASE_DNS_BUDGET: usize = 58;

pub fn runtime_slug(selector: &RoleSelector) -> String {
    selector.namespace.as_ref().map_or_else(
        || selector.name.clone(),
        |namespace| format!("{namespace}__{}", selector.name),
    )
}

pub fn primary_container_name(selector: &RoleSelector) -> String {
    format!("jackin-{}", runtime_slug(selector))
}

pub fn new_container_name(workspace_name: Option<&str>, selector: &RoleSelector) -> String {
    container_name_with_id(workspace_name, selector, &random_instance_id())
}

pub fn container_name_with_id(
    workspace_name: Option<&str>,
    selector: &RoleSelector,
    instance_id: &str,
) -> String {
    let instance_id = compact_component(instance_id, "id");
    let role = compact_component(&selector.name, "role");

    let components = if let Some(workspace_name) = workspace_name {
        let workspace = compact_component(workspace_name, "workspace");
        let budget = ROLE_BASE_DNS_BUDGET - CONTAINER_PREFIX.len() - instance_id.len() - 3;
        let workspace_budget = budget / 2;
        let role_budget = budget - workspace_budget;
        vec![
            CONTAINER_PREFIX.to_string(),
            truncate_component(&workspace, workspace_budget),
            truncate_component(&role, role_budget),
            instance_id,
        ]
    } else {
        let role_budget = ROLE_BASE_DNS_BUDGET - CONTAINER_PREFIX.len() - instance_id.len() - 2;
        vec![
            CONTAINER_PREFIX.to_string(),
            truncate_component(&role, role_budget),
            instance_id,
        ]
    };

    let name = components.join("-");
    debug_assert!(is_dns_label(&name));
    debug_assert!(name.len() <= ROLE_BASE_DNS_BUDGET);
    name
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
    if container_name == primary || container_name.starts_with(&format!("{primary}-clone-")) {
        return true;
    }

    let Some(rest) = container_name.strip_prefix("jackin-") else {
        return false;
    };
    let mut parts = rest.rsplitn(2, '-');
    let Some(_instance_id) = parts.next() else {
        return false;
    };
    let Some(prefix) = parts.next() else {
        return false;
    };
    let role_slug = compact_component(&selector.name, "role");
    prefix
        .rsplit_once('-')
        .map_or(prefix, |(_, visible_role)| visible_role)
        == role_slug
}

pub fn compact_component(input: &str, fallback: &str) -> String {
    let compacted: String = input
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect();
    if compacted.is_empty() {
        fallback.to_string()
    } else {
        compacted
    }
}

pub fn is_dns_label(input: &str) -> bool {
    !input.is_empty()
        && input.len() <= 63
        && input
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && input
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && input
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
}

fn truncate_component(component: &str, max_len: usize) -> String {
    if component.len() <= max_len {
        return component.to_string();
    }
    if max_len <= 4 {
        return short_hash(component, max_len);
    }
    let hash = short_hash(component, 4);
    let keep = max_len - hash.len();
    format!("{}{hash}", &component[..keep])
}

fn short_hash(input: &str, len: usize) -> String {
    let digest = Sha256::digest(input.as_bytes());
    digest
        .iter()
        .flat_map(|byte| {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            [
                HEX[(byte >> 4) as usize] as char,
                HEX[(byte & 0x0f) as usize] as char,
            ]
        })
        .take(len)
        .collect()
}

fn random_instance_id() -> String {
    const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
    let mut value = rand::random::<u64>();
    let mut id = String::with_capacity(INSTANCE_ID_LEN);
    for _ in 0..INSTANCE_ID_LEN {
        id.push(ALPHABET[(value & 0b1_1111) as usize] as char);
        value >>= 5;
    }
    id
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
    fn new_workspace_container_name_is_compact_dns_safe() {
        let selector = RoleSelector::new(Some("chainargos"), "agent-brown");

        let name = container_name_with_id(Some("chainargos-project"), &selector, "k7p9m2xq");

        assert_eq!(name, "jackin-chainargosproject-agentbrown-k7p9m2xq");
        assert!(is_dns_label(&name));
        assert!(is_dns_label(&format!("{name}-dind")));
    }

    #[test]
    fn new_ad_hoc_container_name_omits_workspace_component() {
        let selector = RoleSelector::new(None, "agent-brown");

        let name = container_name_with_id(None, &selector, "k7p9m2xq");

        assert_eq!(name, "jackin-agentbrown-k7p9m2xq");
        assert!(is_dns_label(&name));
    }

    #[test]
    fn long_container_name_fits_dind_dns_budget() {
        let selector = RoleSelector::new(None, "role-name-with-a-very-long-human-friendly-label");

        let name = container_name_with_id(
            Some("workspace-name-with-a-very-long-human-friendly-label"),
            &selector,
            "k7p9m2xq",
        );

        assert!(name.len() <= 58, "{name}");
        assert!(is_dns_label(&format!("{name}-dind")));
    }

    #[test]
    fn class_family_matches_new_unique_names_by_visible_role_component() {
        let selector = RoleSelector::new(Some("chainargos"), "agent-brown");

        assert!(class_family_matches(
            &selector,
            "jackin-chainargosproject-agentbrown-k7p9m2xq"
        ));
        assert!(!class_family_matches(
            &selector,
            "jackin-chainargosproject-agentblue-k7p9m2xq"
        ));
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
