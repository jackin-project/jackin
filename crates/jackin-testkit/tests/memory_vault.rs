// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::SecretBytes;
use jackin_testkit::MemoryVault;
use jackin_usage::accounts::CredentialVault;
use uuid::Uuid;

#[test]
fn the_debug_output_of_the_vault_hides_every_stored_secret()
-> Result<(), Box<dyn std::error::Error>> {
    let vault = MemoryVault::default();
    vault.store(
        Uuid::new_v4(),
        &SecretBytes::new(b"sk-or-fixture-secret".to_vec()),
    )?;
    let shown = format!("{vault:?}");
    assert!(
        !shown.contains("sk-or") && !shown.contains("115") && !shown.contains("fixture"),
        "{shown}"
    );
    Ok(())
}
