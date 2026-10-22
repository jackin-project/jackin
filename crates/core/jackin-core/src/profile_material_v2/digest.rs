// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::error::ProfileCredentialV2Error;

#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProfileCredentialFingerprintV2(String);

impl ProfileCredentialFingerprintV2 {
    pub fn parse_hex(value: &str) -> Result<Self, ProfileCredentialV2Error> {
        if value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Ok(Self(value.to_owned()))
        } else {
            Err(ProfileCredentialV2Error::InvalidFingerprint)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Debug for ProfileCredentialFingerprintV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ProfileCredentialFingerprintV2")
            .field(&"[REDACTED]")
            .finish()
    }
}

impl Serialize for ProfileCredentialFingerprintV2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ProfileCredentialFingerprintV2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)
            .map_err(|_| D::Error::custom("invalid profile credential fingerprint"))?;
        Self::parse_hex(&value)
            .map_err(|_| D::Error::custom("invalid profile credential fingerprint"))
    }
}
