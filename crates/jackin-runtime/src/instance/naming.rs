//! Container naming: generate stable names, class-family matching, and slug derivation.
//!
//! Names encode workspace, role, and a random instance-id component so they
//! are collision-free across concurrent launches of the same role. Not
//! responsible for Docker label writes or image naming — only string
//! derivation.

pub use jackin_core::constants::{
    CONTAINER_PREFIX, CONTAINER_PREFIX_DASH, instance_id_from_container_base,
};
use jackin_core::selector::RoleSelector;
use sha2::{Digest, Sha256};

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
        vec![CONTAINER_PREFIX.to_owned(), instance_id, ws_part, role_part]
    } else {
        let role_budget = ROLE_BASE_DNS_BUDGET - CONTAINER_PREFIX.len() - instance_id.len() - 2;
        vec![
            CONTAINER_PREFIX.to_owned(),
            instance_id,
            truncate_component(&role, role_budget),
        ]
    };

    let name = components.join("-");
    debug_assert!(is_dns_label(&name));
    debug_assert!(name.len() <= ROLE_BASE_DNS_BUDGET);
    name
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
        fallback.to_owned()
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
        return component.to_owned();
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
    let mut digest_hex = hex::encode(digest);
    digest_hex.truncate(len);
    digest_hex
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

/// Docker volume name for the TLS client certificates shared between the
/// `DinD` sidecar (writer) and the role container (reader).
pub(crate) fn dind_certs_volume(container_name: &str) -> String {
    format!("{container_name}-dind-certs")
}

pub(crate) fn dind_container_name(container_name: &str) -> String {
    format!("{container_name}-dind")
}

pub(crate) fn role_network_name(container_name: &str) -> String {
    format!("{container_name}-net")
}

#[cfg(test)]
mod tests;
