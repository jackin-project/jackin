use super::account_key_hash;

#[test]
fn account_key_hash_matches_golden_wire_format() {
    // Locks the host<->container correlation contract: `sha256:` prefix,
    // lowercase hex, NUL field separator. A change here silently breaks
    // usage-row correlation between the host CLI and the in-capsule store.
    assert_eq!(
        account_key_hash("codex", "alexey@example.com"),
        "sha256:aff7fdb5324fe69ac47a6a53a92cd78ebc40d8c2c4057e6113b4ab4d7a398364"
    );
}

#[test]
fn account_key_hash_separator_prevents_field_boundary_collision() {
    // The NUL between provider and label must keep `(ab, c)` distinct from
    // `(a, bc)`; without it the two would hash identically.
    assert_ne!(account_key_hash("ab", "c"), account_key_hash("a", "bc"));
}

#[test]
fn account_key_hash_separator_disambiguates_empty_components() {
    // The NUL must keep an empty label distinct from an empty provider even when
    // one side is empty: `"\0x"` and `"x\0"` are different inputs.
    assert_ne!(account_key_hash("", "x"), account_key_hash("x", ""));
}

#[test]
fn account_key_hash_is_provider_namespaced() {
    assert_ne!(
        account_key_hash("codex", "alexey@example.com"),
        account_key_hash("claude", "alexey@example.com")
    );
}
