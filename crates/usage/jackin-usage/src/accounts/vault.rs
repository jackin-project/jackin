// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;

use jackin_core::SecretBytes;
use keyring::Entry;
use uuid::Uuid;

use super::ConnectionError;

pub trait CredentialVault: Debug + Send + Sync {
    fn store(&self, slot: Uuid, secret: &SecretBytes) -> Result<(), ConnectionError>;
    fn load(&self, slot: Uuid) -> Result<SecretBytes, ConnectionError>;
    fn delete(&self, slot: Uuid) -> Result<(), ConnectionError>;
}

#[derive(Debug)]
pub(super) struct SystemVault;

fn entry(slot: Uuid) -> Result<Entry, ConnectionError> {
    Entry::new("org.jackin.usage.accounts", &slot.to_string()).map_err(|_| ConnectionError::Vault)
}

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
                keyring::Error::NoEntry => ConnectionError::MissingCredential,
                _ => ConnectionError::Vault,
            })
    }

    fn delete(&self, slot: Uuid) -> Result<(), ConnectionError> {
        match entry(slot)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(ConnectionError::Vault),
        }
    }
}
