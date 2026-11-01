// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{AccountName, CodexAuthMode, ProviderName};
use jackin_openshell::{GatewayError, ProfileSync};
use thiserror::Error;

use crate::credentials::CredentialError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountSync {
    HostOnly,
    ProviderReady {
        provider: ProviderName,
        profile: ProfileSync,
    },
}

#[derive(Debug, Error)]
pub enum AccountError {
    #[error(transparent)]
    Credential(#[from] CredentialError),
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error("account {account:?} uses {mode:?}, which is not supported yet")]
    Unsupported {
        account: AccountName,
        mode: CodexAuthMode,
    },
}
