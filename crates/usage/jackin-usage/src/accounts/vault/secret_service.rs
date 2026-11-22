// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use gio::Cancellable;
use jackin_core::{Secret, SecretBytes};
use libsecret::prelude::*;
use libsecret::{
    Collection, CollectionFlags, Item, ItemCreateFlags, SearchFlags, Service, ServiceFlags, Value,
};
use uuid::Uuid;

use super::{ConnectionError, CredentialVault, SystemVault};

const SERVICE: &str = "org.jackin.usage.accounts";
const ENCRYPTED: &str = "dh-ietf1024-sha256-aes128-cbc-pkcs7";
const PREFIX: &[u8] = b"jackin-usage-base64-v1:";

fn collection() -> Result<Collection, ConnectionError> {
    let session = Service::sync(ServiceFlags::OPEN_SESSION, Cancellable::NONE)
        .map_err(|_| ConnectionError::Vault)?;
    if session.session_algorithms().as_deref() != Some(ENCRYPTED) {
        return Err(ConnectionError::Vault);
    }
    let collection = Collection::for_alias_sync(
        Some(&session),
        "default",
        CollectionFlags::NONE,
        Cancellable::NONE,
    )
    .map_err(|_| ConnectionError::Vault)?
    .ok_or(ConnectionError::Vault)?;
    if collection.is_locked() {
        return Err(ConnectionError::Vault);
    }
    Ok(collection)
}

fn attrs(slot: &str) -> HashMap<&str, &str> {
    HashMap::from([("service", SERVICE), ("username", slot)])
}

fn item(slot: Uuid) -> Result<Option<Item>, ConnectionError> {
    let collection = collection()?;
    let slot = slot.to_string();
    let mut found = collection
        .search_sync(None, attrs(&slot), SearchFlags::ALL, Cancellable::NONE)
        .map_err(|_| ConnectionError::Vault)?;
    if found.len() > 1 || found.first().is_some_and(ItemExt::is_locked) {
        return Err(ConnectionError::Vault);
    }
    Ok(found.pop())
}

impl CredentialVault for SystemVault {
    fn store(&self, slot: Uuid, secret: &SecretBytes) -> Result<(), ConnectionError> {
        let collection = collection()?;
        let slot = slot.to_string();
        let mut text = String::from("jackin-usage-base64-v1:");
        STANDARD.encode_string(secret.expose(), &mut text);
        let guarded = Secret::new(text);
        let value = Value::new(guarded.expose(), "text/plain");
        Item::create_sync(
            &collection,
            None,
            attrs(&slot),
            "jackin usage account",
            &value,
            ItemCreateFlags::REPLACE,
            Cancellable::NONE,
        )
        .map(|_| ())
        .map_err(|_| ConnectionError::Vault)
    }
    fn load(&self, slot: Uuid) -> Result<SecretBytes, ConnectionError> {
        let item = item(slot)?.ok_or(ConnectionError::MissingCredential)?;
        item.load_secret_sync(Cancellable::NONE)
            .map_err(|_| ConnectionError::Vault)?;
        let raw = SecretBytes::new(item.secret().ok_or(ConnectionError::Vault)?.get());
        match raw.expose().strip_prefix(PREFIX) {
            Some(encoded) => STANDARD
                .decode(encoded)
                .map(SecretBytes::new)
                .map_err(|_| ConnectionError::Vault),
            None => Ok(raw),
        }
    }
    fn delete(&self, slot: Uuid) -> Result<(), ConnectionError> {
        if let Some(item) = item(slot)? {
            item.delete_sync(Cancellable::NONE)
                .map_err(|_| ConnectionError::Vault)?;
        }
        Ok(())
    }
}
