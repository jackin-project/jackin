// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use jackin_core::SecretBytes;
use jackin_usage::accounts::{ConnectionError, CredentialVault};
use uuid::Uuid;

#[derive(Default)]
pub struct MemoryVault {
    values: Mutex<HashMap<Uuid, Vec<u8>>>,
}

impl std::fmt::Debug for MemoryVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MemoryVault(<redacted>)")
    }
}

impl MemoryVault {
    #[must_use]
    pub fn shared() -> Arc<dyn CredentialVault> {
        Arc::new(Self::default())
    }
}

impl CredentialVault for MemoryVault {
    fn store(&self, slot: Uuid, secret: &SecretBytes) -> Result<(), ConnectionError> {
        self.values
            .lock()
            .map_err(|_| ConnectionError::Vault)?
            .insert(slot, secret.expose().to_vec());
        Ok(())
    }

    fn load(&self, slot: Uuid) -> Result<SecretBytes, ConnectionError> {
        self.values
            .lock()
            .map_err(|_| ConnectionError::Vault)?
            .get(&slot)
            .cloned()
            .map(SecretBytes::new)
            .ok_or(ConnectionError::MissingCredential)
    }

    fn delete(&self, slot: Uuid) -> Result<(), ConnectionError> {
        self.values
            .lock()
            .map_err(|_| ConnectionError::Vault)?
            .remove(&slot);
        Ok(())
    }
}
