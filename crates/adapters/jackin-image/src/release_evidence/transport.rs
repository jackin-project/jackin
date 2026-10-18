// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, ensure};

pub(super) trait Transport {
    async fn get(&self, url: &str, limit: usize) -> Result<Vec<u8>>;
}

pub(super) struct GithubTransport(reqwest::Client);

impl GithubTransport {
    pub(super) fn new() -> Result<Self> {
        Ok(Self(
            reqwest::Client::builder()
                .user_agent(concat!("jackin/", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(60))
                .build()?,
        ))
    }
}

impl Transport for GithubTransport {
    async fn get(&self, url: &str, limit: usize) -> Result<Vec<u8>> {
        crate::telemetry_boundary::download_request(
            if url.starts_with("https://api.github.com/") {
                crate::telemetry_boundary::DownloadRoute::CapsuleAttestation
            } else {
                crate::telemetry_boundary::DownloadRoute::CapsuleEvidence
            },
            url,
            async {
                let mut response = self.0.get(url)
                    .header("Accept", "application/vnd.github+json")
                    .header("X-GitHub-Api-Version", "2022-11-28")
                    .send().await.map_err(reqwest::Error::without_url).context("fetching required capsule release evidence")?;
                ensure!(response.status().is_success(), "required capsule evidence unavailable (HTTP {}); public GitHub API may be rate-limited. Retry after the rate limit resets, or build/cache a matching local capsule. Verification cannot be skipped", response.status());
                ensure!(response.content_length().is_none_or(|n| n <= limit as u64), "capsule evidence exceeds size limit");
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(reqwest::Error::without_url)? {
                    ensure!(chunk.len() <= limit.saturating_sub(bytes.len()), "capsule evidence exceeds size limit");
                    bytes.extend_from_slice(&chunk);
                }
                Ok(bytes)
            },
        ).await
    }
}
