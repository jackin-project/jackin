// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;

use jackin_core::SecretBytes;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use keyring_core::Entry;
use std::sync::Arc;
use uuid::Uuid;

use super::ConnectionError;

pub trait CredentialVault: Debug + Send + Sync {
    fn store(&self, slot: Uuid, secret: &SecretBytes) -> Result<(), ConnectionError>;
    fn load(&self, slot: Uuid) -> Result<SecretBytes, ConnectionError>;
    fn delete(&self, slot: Uuid) -> Result<(), ConnectionError>;
}

#[derive(Debug)]
struct SystemVault;

pub fn system_vault() -> Arc<dyn CredentialVault> {
    Arc::new(SystemVault)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn entry(slot: Uuid) -> Result<Entry, ConnectionError> {
    #[cfg(target_os = "macos")]
    use apple_native_keyring_store::keychain::Store;
    #[cfg(target_os = "windows")]
    use windows_native_keyring_store::Store;
    let store: Arc<keyring_core::CredentialStore> =
        Store::new().map_err(|_| ConnectionError::Vault)?;
    Entry::new_in_store(&store, "org.jackin.usage.accounts", &slot.to_string())
        .map_err(|_| ConnectionError::Vault)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl CredentialVault for SystemVault {
    fn store(&self, slot: Uuid, secret: &SecretBytes) -> Result<(), ConnectionError> {
        entry(slot)?
            .set_secret(secret.expose())
            .map_err(|_| ConnectionError::Vault)
    }

    fn load(&self, slot: Uuid) -> Result<SecretBytes, ConnectionError> {
        entry(slot)?
            .get_secret()
            .map(SecretBytes::new)
            .map_err(|e| match e {
                keyring_core::Error::NoEntry => ConnectionError::MissingCredential,
                _ => ConnectionError::Vault,
            })
    }

    fn delete(&self, slot: Uuid) -> Result<(), ConnectionError> {
        match entry(slot)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(_) => Err(ConnectionError::Vault),
        }
    }
}

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
mod secret_service;
