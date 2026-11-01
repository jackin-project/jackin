// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Secret(Box<str>);

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecretBytes(Box<[u8]>);

#[derive(Clone, Debug)]
pub struct Redactor {}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TerminalText(String);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBytes(<redacted>)")
    }
}
