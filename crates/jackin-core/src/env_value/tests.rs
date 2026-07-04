//! Tests for `env_value`.

use super::*;

#[test]
fn op_ref_deserializes_op_uri() {
    let json = r#"{"op":"op://vault/item/field","path":"Vault/Item/Field"}"#;

    let value = serde_json::from_str::<EnvValue>(json);

    let Ok(EnvValue::OpRef(reference)) = value else {
        panic!("expected op ref to deserialize");
    };
    assert_eq!(reference.op, "op://vault/item/field");
    assert_eq!(reference.path, "Vault/Item/Field");
    assert_eq!(reference.account, None);
    assert!(!reference.on_demand);
}

#[test]
fn op_ref_rejects_non_op_uri() {
    let json = r#"{"op":"not-op://vault/item/field","path":"Vault/Item/Field"}"#;

    let result = serde_json::from_str::<OpRef>(json);

    let Err(err) = result else {
        panic!("expected invalid op ref to fail");
    };
    assert!(
        err.to_string()
            .contains("op reference must start with op://"),
        "{err}"
    );
}
