// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn env_rows_omits_account_owned_sentinel_values() {
    let mut env = BTreeMap::new();
    env.insert(
        "ANTHROPIC_API_KEY".to_owned(),
        EnvValue::Plain("cli-list-sentinel".into()),
    );
    env.insert("PROJECT_ENV".to_owned(), EnvValue::Plain("visible".into()));

    let rows = env_rows(&env);
    assert_eq!(rows, vec![("PROJECT_ENV".into(), "visible".into(), false)]);
    assert!(!format!("{rows:?}").contains("cli-list-sentinel"));
}

#[test]
fn unresolved_op_ref_keeps_uri_verbatim_with_a_display_breadcrumb() {
    let reference = unresolved_op_ref("op://Vault/Item/key", false).unwrap();
    assert_eq!(reference.op, "op://Vault/Item/key");
    assert_eq!(reference.path, "Vault/Item/key");
    assert_eq!(reference.account, None);
    assert!(!reference.on_demand);

    let reference = unresolved_op_ref("op://Vault/Item/key", true).unwrap();
    assert!(reference.on_demand);

    let sectioned = unresolved_op_ref("op://Vault/Item/Section/key?attribute=otp", false).unwrap();
    assert_eq!(sectioned.op, "op://Vault/Item/Section/key?attribute=otp");
    assert_eq!(sectioned.path, "Vault/Item/Section/key");
}

#[test]
fn unresolved_op_ref_rejects_malformed_uris() {
    for value in [
        "op://",
        "op://vault",
        "op://vault/item",
        "op://vault//field",
    ] {
        assert!(
            unresolved_op_ref(value, false).is_err(),
            "{value} must not persist"
        );
    }
}

#[test]
fn unavailable_op_persists_the_op_ref_unresolved() {
    let value = resolve_env_value_for_cli_with_runner("op://Vault/Item/key", false, None).unwrap();
    let EnvValue::OpRef(reference) = value else {
        panic!("an op:// value must persist as an OpRef, got {value:?}");
    };
    assert_eq!(reference.op, "op://Vault/Item/key");
    assert_eq!(reference.path, "Vault/Item/key");
}

#[test]
fn available_op_canonicalizes_the_op_ref() {
    let stub = stub_op();
    let value =
        resolve_env_value_for_cli_with_runner("op://Vault/Item/key", false, Some(&stub)).unwrap();
    let EnvValue::OpRef(reference) = value else {
        panic!("an op:// value must persist as an OpRef, got {value:?}");
    };
    assert_eq!(reference.op, "op://vault-uuid/item-uuid/field-id");
    assert_eq!(reference.path, "Vault/Item/key");
}

#[test]
fn available_op_reports_an_unknown_vault_honestly() {
    let stub = stub_op();
    let error =
        resolve_env_value_for_cli_with_runner("op://NoSuchVault/Item/key", false, Some(&stub))
            .unwrap_err()
            .to_string();
    assert!(error.contains("NoSuchVault"), "{error}");
}

#[test]
fn op_runner_failure_propagates_the_underlying_error() {
    let temp = tempfile::tempdir().unwrap();
    let shim = temp.path().join("op");
    std::fs::write(
        &shim,
        "#!/bin/sh\n\
             if [ \"$1\" = \"--version\" ]; then echo 2.39.0; exit 0; fi\n\
             echo '[ERROR] 2026/01/01 you are not signed in; run `op signin`' >&2\n\
             exit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let runner = jackin_env::OpCli::with_binary(shim.to_string_lossy().into_owned());
    let error = resolve_env_value_for_cli_with_runner("op://Vault/Item/key", false, Some(&runner))
        .unwrap_err()
        .to_string();
    assert!(error.contains("not signed in"), "{error}");
}

#[test]
fn unresolved_ref_resolution_fails_honestly_without_op() {
    let value = EnvValue::OpRef(unresolved_op_ref("op://Vault/Item/key", false).unwrap());
    let runner = jackin_env::OpCli::with_binary("/nonexistent-op-binary-jackin-test".to_owned());
    let error = jackin_env::resolve_env_value("test-layer", "SECRET", &value, &runner, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("SECRET"), "{error}");
    assert!(error.contains("1Password"), "{error}");
}

#[test]
fn cli_resolution_follows_live_op_availability() {
    let probe = jackin_env::OpCli::new();
    let available = jackin_env::OpRunner::probe(&probe).is_ok();
    let result = resolve_env_value_for_cli("op://NoSuchVaultForJackinTest/NoSuchItem/key", false);
    if available {
        assert!(
            result.is_err(),
            "live `op` must attempt resolution, not silently persist"
        );
    } else {
        assert!(matches!(result.unwrap(), EnvValue::OpRef(_)));
    }
}
