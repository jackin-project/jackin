// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared HTTP helpers for binary acquisition.
//!
//! `agent_binary` and `capsule_binary` both fetch small release metadata over
//! HTTP and download multi-MB binaries from Range-supporting CDNs. This module
//! owns the reqwest text-GET client, the GET-to-`String` shape, and the
//! fast-down parallel-download pipeline (which builds its own client) so the
//! two callers stay in lockstep — tuning the chunk count or timeout happens
//! here once.

use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

use crate::DockerError;
use anyhow::{Context, Result};
use fast_down::{
    Event, Proxy,
    fast_puller::{FastDownPuller, FastDownPullerOptions, build_client},
    file::MmapFilePusher,
    http::Prefetch,
    multi::{self, download_multi},
};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT as USER_AGENT_HEADER};

/// Deadline for a small metadata/API GET (latest version, manifest, `.sha256`).
const TEXT_GET_TIMEOUT: Duration = Duration::from_secs(30);
/// Connection-establishment deadline shared by every client here.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Overall ceiling for one parallel binary download. fast-down classifies every
/// bounded-range chunk error as recoverable and retries with no attempt cap, so
/// without this a persistently-failing CDN loops forever and wedges the launch.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_mins(5);
/// Parallel chunk connections per download.
const DOWNLOAD_CONCURRENCY: usize = 8;

/// `User-Agent` sent on every HTTP request.
///
/// GitHub's API rejects UA-less requests at the edge with HTTP 403 ("Request
/// forbidden by administrative rules") before any auth or rate-limit logic
/// runs — reqwest does not set a default UA — so leaving this off silently
/// breaks Codex/Opencode release metadata even when a valid `gh` token is attached.
pub const USER_AGENT: &str = concat!("jackin/", env!("JACKIN_VERSION"));

/// Build a reqwest client carrying `headers` as defaults.
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

/// GET `url` with `client`, erroring on a non-success status, and return the
/// body as text.
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

/// GET `url` with a default (header-less) client and return the body as text.
pub async fn fetch_text(url: &str) -> Result<String> {
    get_text(default_http_client()?, url).await
}

/// Download `url` to `dest` using fast-down parallel chunks.
///
/// Work-stealing across `DOWNLOAD_CONCURRENCY` connections with mmap writes. All
/// CDNs jackin❯ pulls from support Range requests; bail fast if one somehow does
/// not. Creates `dest`'s parent directory when missing. Bounded by
/// `DOWNLOAD_TIMEOUT` — completeness/integrity of the result is the caller's
/// SHA-256 check, since the mmap pre-sizes the file (a dropped chunk leaves a
/// zeroed hole, not a short file).
pub async fn download_parallel(url: &str, dest: &Path) -> Result<()> {
    // TLS/proxy posture, bound once so the prefetch client and the chunk puller
    // share one auditable security default instead of restating bare positional
    // `false`s at two call sites.
    let proxy = Proxy::System;
    let accept_invalid_certs = false;
    let accept_invalid_hostnames = false;

    let parsed = reqwest::Url::parse(url).with_context(|| format!("invalid URL {url}"))?;
    // No auth headers: every download URL is a public CDN asset (GitHub release
    // browser_download_url, Claude/Amp/Kimi CDNs), unlike the rate-limited
    // GitHub API metadata path that carries a Bearer token. UA is set for the
    // same reason as the metadata client — bot/WAF rules at some CDNs reject
    // UA-less clients.
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT_HEADER, HeaderValue::from_static(USER_AGENT));
    let client = build_client(
        &headers,
        proxy,
        accept_invalid_certs,
        accept_invalid_hostnames,
        None,
    )
    .context("building HTTP client")?;
    let (info, _resp) = client.prefetch(parsed).await.map_err(|(err, _)| {
        anyhow::Error::from(DockerError::Prefetch {
            url: url.to_owned(),
            detail: format!("{err:?}"),
        })
    })?;
    jackin_diagnostics::debug_log!(
        "download",
        "{url}: size={}, parallel={}",
        info.size,
        info.fast_download
    );
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
        file_id: info.file_id,
        // resp: None — every chunk (including the first) issues its own ranged
        // GET rather than reusing the prefetch body; prefetch is consumed only
        // for the size + Range-support probe above.
        resp: None,
        available_ips: Arc::from([]),
    })
    .context("building parallel downloader")?;
    let pusher = MmapFilePusher::new(file, info.size, false)
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
    // Drain the event stream, then await the writer — bounded by an overall
    // deadline. A persistently-failing chunk retries forever (every bounded
    // range is classified recoverable), so both the drain and the join can hang
    // indefinitely; the timeout + abort is the only ceiling. Per-chunk errors
    // are recoverable and high-frequency, so they go on the gated `debug_log!`
    // tier, not the always-on diagnostics log.
    let drive = async {
        while let Ok(event) = result.event_chain.recv().await {
            match event {
                Event::PullError(id, err) => {
                    jackin_diagnostics::debug_log!("download", "worker {id} pull error: {err:?}");
                }
                Event::PushError(_, _, err) | Event::FlushError(err) => {
                    jackin_diagnostics::debug_log!("download", "write error: {err}");
                }
                _ => {}
            }
        }
        result.join().await.map_err(|e| {
            anyhow::Error::from(DockerError::DownloadTaskPanicked {
                url: url.to_owned(),
                detail: e.to_string(),
            })
        })
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
