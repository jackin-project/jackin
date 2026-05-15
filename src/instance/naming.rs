use crate::selector::RoleSelector;
use sha2::{Digest, Sha256};

pub(crate) const CONTAINER_PREFIX: &str = "jk";
pub(crate) const CONTAINER_PREFIX_DASH: &str = "jk-";
const INSTANCE_ID_LEN: usize = 8;
const ROLE_BASE_DNS_BUDGET: usize = 58;

pub fn runtime_slug(selector: &RoleSelector) -> String {
    selector.namespace.as_ref().map_or_else(
        || selector.name.clone(),
        |namespace| format!("{namespace}_{}", selector.name),
    )
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
        let (ws_part, role_part) = if workspace.len() + role.len() <= budget {
            (workspace, role)
        } else {
            let workspace_budget = budget / 2;
            let role_budget = budget - workspace_budget;
            (
                truncate_component(&workspace, workspace_budget),
                truncate_component(&role, role_budget),
            )
        };
        vec![
            CONTAINER_PREFIX.to_string(),
            instance_id,
            ws_part,
            role_part,
        ]
    } else {
        let role_budget = ROLE_BASE_DNS_BUDGET - CONTAINER_PREFIX.len() - instance_id.len() - 2;
        vec![
            CONTAINER_PREFIX.to_string(),
            instance_id,
            truncate_component(&role, role_budget),
        ]
    };

    let name = components.join("-");
    debug_assert!(is_dns_label(&name));
    debug_assert!(name.len() <= ROLE_BASE_DNS_BUDGET);
    name
}

/// Extract the instance-ID component from a container name.
///
/// Returns `None` when the name does not start with `jk-` or has no `-` after
/// the id component. Used by both manifest construction (the stored
/// `instance_id` field) and operator display rendering — one parser owns the
/// shape `jk-<id>[-<workspace>]-<role>` produced by [`new_container_name`].
#[must_use]
pub fn instance_id_from_container_base(container_base: &str) -> Option<&str> {
    container_base
        .strip_prefix(CONTAINER_PREFIX_DASH)?
        .split_once('-')
        .map(|(id, _)| id)
}

/// Recognize names of the shape `jk-<id>[-<workspace>]-<role>`
/// produced by `new_container_name`. Scoping hook for `purge_class_data`.
pub fn class_family_matches(selector: &RoleSelector, container_name: &str) -> bool {
    class_family_matches_with_slug(&compact_component(&selector.name, "role"), container_name)
}

/// Loop-friendly variant of [`class_family_matches`] for callers that
/// precompute the slug once across many candidates — avoids one
/// [`compact_component`] allocation per comparison.
#[must_use]
pub fn class_family_matches_with_slug(role_slug: &str, container_name: &str) -> bool {
    let Some(rest) = container_name.strip_prefix(CONTAINER_PREFIX_DASH) else {
        return false;
    };
    let Some((_, after_id)) = rest.split_once('-') else {
        return false;
    };
    after_id.rsplit_once('-').map_or(after_id, |(_, role)| role) == role_slug
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
    let mut hex = hex_lower(&digest);
    hex.truncate(len);
    hex
}

/// Lowercase hex encoding of arbitrary bytes.
pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
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
    fn new_workspace_container_name_is_compact_dns_safe() {
        let selector = RoleSelector::new(Some("chainargos"), "agent-brown");

        let name = container_name_with_id(Some("chainargos-project"), &selector, "k7p9m2xq");

        assert_eq!(name, "jk-k7p9m2xq-chainargosproject-agentbrown");
        assert!(is_dns_label(&name));
        assert!(is_dns_label(&format!("{name}-dind")));
    }

    #[test]
    fn new_ad_hoc_container_name_omits_workspace_component() {
        let selector = RoleSelector::new(None, "agent-brown");

        let name = container_name_with_id(None, &selector, "k7p9m2xq");

        assert_eq!(name, "jk-k7p9m2xq-agentbrown");
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
            "jk-k7p9m2xq-chainargosproject-agentbrown"
        ));
        assert!(!class_family_matches(
            &selector,
            "jk-k7p9m2xq-chainargosproject-agentblue"
        ));
    }

    #[test]
    fn class_family_matches_distinguishes_role_substrings() {
        // A role named `brown` must not match a container whose role
        // component is `agentbrown` (the longer name happens to end
        // in `brown`). Important for `purge_class_data` blast radius.
        let brown = RoleSelector::new(None, "brown");
        assert!(!class_family_matches(&brown, "jk-k7p9m2xq-agentbrown",));
        let agentbrown = RoleSelector::new(None, "agentbrown");
        assert!(!class_family_matches(&agentbrown, "jk-k7p9m2xq-brown",));
        assert!(class_family_matches(&agentbrown, "jk-k7p9m2xq-agentbrown",));
    }

    #[test]
    fn instance_id_from_container_base_extracts_second_component() {
        assert_eq!(
            instance_id_from_container_base("jk-k7p9m2xq-workspace-agentsmith"),
            Some("k7p9m2xq")
        );
        assert_eq!(
            instance_id_from_container_base("jk-k7p9m2xq-agentsmith"),
            Some("k7p9m2xq")
        );
        assert_eq!(instance_id_from_container_base("nojkprefix-k7p9m2xq"), None);
        assert_eq!(instance_id_from_container_base("jk-noid"), None);
    }

    #[test]
    fn no_workspace_long_role_fits_dns_budget() {
        let selector = RoleSelector::new(None, "role-name-with-a-very-long-human-friendly-label");

        let name = container_name_with_id(None, &selector, "k7p9m2xq");

        assert!(name.len() <= 58, "{name}");
        assert!(is_dns_label(&format!("{name}-dind")));
    }
}
