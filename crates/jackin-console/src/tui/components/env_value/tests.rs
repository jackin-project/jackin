use super::secret_display;
use crate::tui::components::editor_rows::SecretValueDisplay;

#[test]
fn secret_display_uses_plain_value_or_op_path() {
    let plain = jackin_core::EnvValue::Plain("literal".to_owned());
    assert!(matches!(
        secret_display(&plain),
        SecretValueDisplay::Plain("literal")
    ));

    let op_ref = jackin_core::EnvValue::OpRef(jackin_core::OpRef {
        op: "op://vault/item/field".to_owned(),
        path: "Vault/Item/Field".to_owned(),
        account: None,
        on_demand: false,
    });
    assert!(matches!(
        secret_display(&op_ref),
        SecretValueDisplay::OpRefPath("Vault/Item/Field")
    ));
}
