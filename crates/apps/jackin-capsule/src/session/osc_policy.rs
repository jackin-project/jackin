// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate as _;

#[must_use]
pub fn osc8_uri_is_safe(uri: &str) -> bool {
    if uri.is_empty() {
        return true;
    }

    let bytes = uri.trim().as_bytes();
    [b"http://".as_slice(), b"https://", b"mailto:"]
        .iter()
        .any(|scheme| {
            bytes
                .get(..scheme.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
        })
}

#[must_use]
pub fn parse_osc7(payload: &str) -> Option<String> {
    let url = url::Url::parse(payload).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    url.to_file_path()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

const ENV_OSC52: &str = "JACKIN_OSC52";
const ENV_OSC_TITLE: &str = "JACKIN_OSC_TITLE";
const ENV_OSC_NOTIFY: &str = "JACKIN_OSC_NOTIFY";
const ENV_OSC_HYPERLINK: &str = "JACKIN_OSC_HYPERLINK";

#[derive(Debug, Clone, Copy)]
pub struct OscPolicy {
    flags: u8,
}

const ALLOW_TITLE: u8 = 1 << 0;
const ALLOW_OSC52: u8 = 1 << 1;
const ALLOW_NOTIFY: u8 = 1 << 2;
const ALLOW_HYPERLINK: u8 = 1 << 3;

impl Default for OscPolicy {
    fn default() -> Self {
        Self {
            flags: ALLOW_TITLE | ALLOW_NOTIFY | ALLOW_HYPERLINK,
        }
    }
}

impl OscPolicy {
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Self {
        Self {
            flags: (if is_deny(lookup(ENV_OSC_TITLE).as_deref()) {
                0
            } else {
                ALLOW_TITLE
            }) | (if is_allow(lookup(ENV_OSC52).as_deref()) {
                ALLOW_OSC52
            } else {
                0
            }) | (if is_deny(lookup(ENV_OSC_NOTIFY).as_deref()) {
                0
            } else {
                ALLOW_NOTIFY
            }) | (if is_deny(lookup(ENV_OSC_HYPERLINK).as_deref()) {
                0
            } else {
                ALLOW_HYPERLINK
            }),
        }
    }

    #[must_use]
    pub fn allow_title(self) -> bool {
        self.flags & ALLOW_TITLE != 0
    }
    #[must_use]
    pub fn allow_osc52(self) -> bool {
        self.flags & ALLOW_OSC52 != 0
    }
    #[must_use]
    pub fn allow_notify(self) -> bool {
        self.flags & ALLOW_NOTIFY != 0
    }
    #[must_use]
    pub fn allow_hyperlink(self) -> bool {
        self.flags & ALLOW_HYPERLINK != 0
    }

    #[doc(hidden)]
    #[must_use]
    pub fn for_test_deny_all() -> Self {
        Self { flags: 0 }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn for_test_allow_all() -> Self {
        Self {
            flags: ALLOW_TITLE | ALLOW_OSC52 | ALLOW_NOTIFY | ALLOW_HYPERLINK,
        }
    }
}

fn is_allow(value: Option<&str>) -> bool {
    matches!(value, Some("allow"))
}

fn is_deny(value: Option<&str>) -> bool {
    matches!(value, Some("deny" | "off" | "no"))
}

#[cfg(test)]
mod tests;
