//! Shared HTTP helpers for binary acquisition.
//!
//! `agent_binary` and `capsule_binary` both fetch small release metadata over
//! HTTP and download multi-MB binaries from Range-supporting CDNs. This module
//! owns the single copy of the reqwest client builder, the GET-to-`String`
//! shape, and the fast-down parallel-download pipeline so the two callers stay
//! in lockstep — tuning the chunk count or timeout happens here once.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use fast_down::{
    Event, Proxy,
    fast_puller::{FastDownPuller, FastDownPullerOptions, build_client},
    file::MmapFilePusher,
    http::Prefetch,
    multi::{self, download_multi},
};
use reqwest::header::HeaderMap;

/// Build a reqwest client carrying `headers` as defaults.
pub fn http_client(headers: HeaderMap) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .context("building HTTP client")
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
    anyhow::ensure!(status.is_success(), "{url} failed: HTTP {status}");
    resp.text()
        .await
        .with_context(|| format!("{url} body is not valid UTF-8"))
}

/// GET `url` with a default (header-less) client and return the body as text.
pub async fn fetch_text(url: &str) -> Result<String> {
    get_text(&http_client(HeaderMap::new())?, url).await
}

/// Download `url` to `dest` using fast-down parallel chunks.
///
/// Work-stealing across 8 connections with mmap writes. All CDNs jackin' pulls
/// from support Range requests; bail fast if one somehow does not. Creates
/// `dest`'s parent directory when missing.
pub async fn download_parallel(url: &str, dest: &Path) -> Result<()> {
    let parsed = reqwest::Url::parse(url).with_context(|| format!("invalid URL {url}"))?;
    let headers = HeaderMap::new();
    let client = build_client(&headers, Proxy::System, false, false, None)
        .context("building HTTP client")?;
    let (info, _resp) = client
        .prefetch(parsed)
        .await
        .map_err(|(err, _)| anyhow::anyhow!("prefetch {url}: {err:?}"))?;
    crate::debug_log!(
        "download",
        "{url}: size={}, parallel={}",
        info.size,
        info.fast_download
    );
    anyhow::ensure!(
        info.fast_download,
        "server at {url} does not support Range requests; cannot download in parallel"
    );
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
        proxy: Proxy::System,
        accept_invalid_certs: false,
        accept_invalid_hostnames: false,
        file_id: info.file_id,
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
            concurrent: 8,
            retry_gap: Duration::from_millis(500),
            push_queue_cap: 1024,
            pull_timeout: Duration::from_secs(30),
            min_chunk_size: 8 * 1024 * 1024,
            max_speculative: 3,
        },
    );
    // Per-chunk errors are recoverable (fast-down retries) and high-frequency,
    // so they belong on the verbose `cdebug!` tier, not the always-on log.
    while let Ok(event) = result.event_chain.recv().await {
        match event {
            Event::PullError(id, err) => {
                crate::debug_log!("download", "worker {id} pull error: {err:?}");
            }
            Event::PushError(_, _, err) | Event::FlushError(err) => {
                crate::debug_log!("download", "write error: {err}");
            }
            _ => {}
        }
    }
    result
        .join()
        .await
        .map_err(|e| anyhow::anyhow!("download task panicked for {url}: {e}"))
}
