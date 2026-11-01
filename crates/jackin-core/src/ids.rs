// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU64;

use serde::Serialize;
use thiserror::Error;

macro_rules! string_ids {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
            pub struct $name(String);
        )+
    };
}

string_ids!(
    WorkspaceName,
    RoleName,
    GatewayName,
    BuilderName,
    OpenShellWorkspace,
    LaunchId,
    EnvName,
    ChatgptAccountId,
    BranchName,
    CommitId,
    Sha256,
    OpReference,
    AccountName,
    ProviderName,
    ProfileId,
    SandboxSlug,
    NameSuffix,
    SandboxName,
    ImageLabel,
    ImageTag,
    ModelName,
);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct AttemptId(NonZeroU64);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ImageId(Sha256);

impl SandboxSlug {
    pub const MAX_LEN: usize = 7;
}

impl NameSuffix {
    pub const LEN: usize = 4;
}

impl SandboxName {
    pub const MAX_LEN: usize = 19;
}

impl AttemptId {
    pub const FIRST: AttemptId = AttemptId(NonZeroU64::MIN);
}

#[derive(Debug, Error)]
pub enum IdError {
    #[error("{kind} is empty")]
    Empty { kind: &'static str },
    #[error("{kind} is {len} characters long and the limit is {max}")]
    TooLong {
        kind: &'static str,
        max: usize,
        len: usize,
    },
    #[error("{kind} has the invalid character {ch:?} at index {index}")]
    InvalidChar {
        kind: &'static str,
        ch: char,
        index: usize,
    },
    #[error("{kind} must start and end with an alphanumeric character")]
    BadEdge { kind: &'static str },
    #[error("{kind} must be {expected}")]
    BadFormat {
        kind: &'static str,
        expected: &'static str,
    },
}
