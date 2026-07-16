// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{event::Rejection, schema};

pub fn validate_key(key: &str) -> Result<(), Rejection> {
    let extension = schema::ALL_KEYS.contains(&key);
    let standard = schema::attrs::std_attrs::ALL_KEYS.contains(&key);
    if extension || standard {
        Ok(())
    } else {
        Err(Rejection::UnknownAttribute)
    }
}

#[cfg(test)]
mod tests;
