// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{Clock, EnvSnapshot, FileSystem, HostPath, ProcessRunner, Provider};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct ProviderEndpoints {
    bases: BTreeMap<Provider, String>,
    timeout: Duration,
}

#[derive(Clone, Debug)]
pub struct UsageDeps {
    pub clock: Arc<dyn Clock>,
    pub fs: Arc<dyn FileSystem>,
    pub runner: Arc<dyn ProcessRunner>,
    pub env: EnvSnapshot,
    pub endpoints: ProviderEndpoints,
    pub home: HostPath,
    pub state_dir: HostPath,
}

impl ProviderEndpoints {
    pub fn real() -> ProviderEndpoints {
        ProviderEndpoints {
            bases: BTreeMap::new(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    #[must_use]
    pub fn with(mut self, provider: Provider, base_url: String) -> ProviderEndpoints {
        self.bases.insert(provider, base_url);
        self
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> ProviderEndpoints {
        self.timeout = timeout;
        self
    }

    pub(crate) fn base(&self, provider: Provider) -> Option<&str> {
        self.bases.get(&provider).map(String::as_str)
    }

    pub(crate) fn timeout(&self) -> Duration {
        self.timeout
    }
}
