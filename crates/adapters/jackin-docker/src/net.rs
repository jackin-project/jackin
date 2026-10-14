// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

use crate::DockerError;
use anyhow::{Context, Result};
use fast_down::{
    MmapFilePusher, Proxy,
    fast_puller::{FastDownPuller, FastDownPullerOptions, build_client},
    http::Prefetch,
    multi::{self, download_multi},
};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT as USER_AGENT_HEADER};

const TEXT_GET_TIMEOUT: Duration = Duration::from_secs(30);

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

const DOWNLOAD_TIMEOUT: Duration = Duration::from_mins(5);

const DOWNLOAD_CONCURRENCY: usize = 8;

pub const USER_AGENT: &str = concat!("jackin/", env!("JACKIN_VERSION"));

pub fn http_client(headers: HeaderMap) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .timeout(TEXT_GET_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .context("building HTTP client")
}

fn default_http_client() -> Result<&'static reqwest::Client> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| http_client(HeaderMap::new()).map_err(|error| format!("{error:#}")))
        .as_ref()
        .map_err(|error| DockerError::HttpClientBuild(error.clone()).into())
}

pub async fn get_text(client: &reqwest::Client, url: &str) -> Result<String> {
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(DockerError::HttpStatus {
            url: url.to_owned(),
            status: status.to_string(),
        }
        .into());
    }
    resp.text()
        .await
        .with_context(|| format!("{url} body is not valid UTF-8"))
}

pub async fn fetch_text(url: &str) -> Result<String> {
    get_text(default_http_client()?, url).await
}

pub async fn download_parallel(url: &str, dest: &Path) -> Result<()> {
    let proxy = Proxy::System;
    let accept_invalid_certs = false;
    let accept_invalid_hostnames = false;

    let parsed = reqwest::Url::parse(url).with_context(|| format!("invalid URL {url}"))?;

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT_HEADER, HeaderValue::from_static(USER_AGENT));
    let client = build_client(
        headers.clone(),
        proxy,
        accept_invalid_certs,
        accept_invalid_hostnames,
        false,
        None,
        10,
    )
    .context("building HTTP client")?;
    let (info, _resp) = client.prefetch(parsed).await.map_err(|(err, _)| {
        anyhow::Error::from(DockerError::Prefetch {
            url: url.to_owned(),
            detail: format!("{err:?}"),
        })
    })?;
    if !info.fast_download {
        return Err(DockerError::RangeUnsupported {
            url: url.to_owned(),
        }
        .into());
    }
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("creating parent dir for {}", dest.display()))?;
    }
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .truncate(true)
        .open(dest)
        .await
        .with_context(|| format!("creating download destination {}", dest.display()))?;
    let puller = FastDownPuller::new(FastDownPullerOptions {
        url: info.final_url,
        headers: Arc::new(headers),
        proxy,
        accept_invalid_certs,
        accept_invalid_hostnames,
        cookie_store: false,
        file_id: info.file_id,

        resp: None,
        available_ips: Arc::from([]),
        max_redirects: 10,
    })
    .context("building parallel downloader")?;
    let pusher = MmapFilePusher::new(&file, info.size, false)
        .await
        .context("creating memory-mapped file writer")?;
    let result = download_multi(
        puller,
        pusher,
        multi::DownloadOptions {
            download_chunks: std::iter::once(0..info.size),
            concurrent: DOWNLOAD_CONCURRENCY,
            retry_gap: Duration::from_millis(500),
            push_queue_cap: 1024,
            pull_timeout: Duration::from_secs(30),
            min_chunk_size: 8 * 1024 * 1024,
            max_speculative: 3,
        },
    );

    let drive = async {
        let mut flush_error = None;
        while let Ok(event) = result.event_chain().recv().await {
            match event {
                fast_down::Event::Flushing => flush_error = None,
                fast_down::Event::FlushError(error) => flush_error = Some(error.to_string()),
                _ => {}
            }
        }
        if let Some(detail) = flush_error {
            Err(anyhow::Error::from(DockerError::DownloadFlushFailed {
                url: url.to_owned(),
                detail,
            }))
        } else {
            Ok(())
        }
    };
    let Ok(outcome) = tokio::time::timeout(DOWNLOAD_TIMEOUT, drive).await else {
        result.abort();
        return Err(DockerError::DownloadTimeout {
            url: url.to_owned(),
            timeout: DOWNLOAD_TIMEOUT,
        }
        .into());
    };
    outcome
}

#[cfg(test)]
mod tests;
