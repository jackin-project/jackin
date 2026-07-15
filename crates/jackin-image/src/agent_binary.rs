// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Download and cache agent CLI binaries (Kimi, etc.) for injection into derived images.
//!
//! Fetches from upstream CDN, verifies SHA-256, caches under `~/.jackin/cache/`
//! with a 1-hour TTL. Not responsible for injecting binaries into the Docker
//! build context — callers in `runtime::image` handle that step.

use crate::ImageError;
use crate::binary_artifact::{
    chmod_executable, container_arch, extract_tar_gz_member, hash_file_sha256, is_executable_file,
    parse_sha256_hex, repair_executable_file,
};
use anyhow::{Context, Result};
use jackin_core::{Agent, Clock, JackinPaths, SystemClock};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::sync::OnceCell;

const CACHE_TTL: Duration = Duration::from_hours(1);
const KIMI_DOWNLOAD_BASE_URL: &str = "https://code.kimi.com/kimi-code";
const KIMI_BINARY_BASE_URL: &str = "https://code.kimi.com/kimi-code/binaries";

const GROK_BASE_PRIMARY: &str = "https://x.ai/cli";
const GROK_BASE_FALLBACK: &str = "https://storage.googleapis.com/grok-build-public-artifacts/cli";

static GITHUB_AUTH_TOKEN: OnceCell<Option<String>> = OnceCell::const_new();

#[derive(Debug, Clone)]
pub struct AgentBinary {
    pub agent: Agent,
    pub path: PathBuf,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRelease {
    pub agent: Agent,
    pub version: String,
    pub url: String,
    pub checksum: Option<String>,
    pub archive_member: Option<String>,
}

pub async fn latest_release(paths: &JackinPaths, agent: Agent) -> Option<AgentRelease> {
    if let Some(cached) = read_cached_release_async(paths, agent).await {
        return Some(cached);
    }
    match resolve_latest_release(agent).await {
        Ok(release) => {
            persist_release_cache_async(paths, &release).await;
            Some(release)
        }
        Err(error) => {
            record(
                "warning",
                &format!(
                    "{} latest version lookup failed; using cached metadata if available: {error:#}",
                    agent.slug()
                ),
            );
            newest_cached_executable_release_async(paths, agent)
                .await
                .map(|(_, release, _)| release)
        }
    }
}

pub async fn ensure_available(paths: &JackinPaths, agent: Agent) -> Result<AgentBinary> {
    ensure_available_impl(paths, agent, true).await
}

async fn ensure_available_impl(
    paths: &JackinPaths,
    agent: Agent,
    install_stub: bool,
) -> Result<AgentBinary> {
    #[cfg(not(test))]
    let _ = install_stub;
    let stub_path = test_stub_path(paths, agent);
    #[cfg(test)]
    if install_stub && !is_executable_file(&stub_path) {
        install_test_stub(paths, agent).context("installing in-process agent binary test stub")?;
    }
    if is_executable_file_async(&stub_path).await {
        record(
            "agent_binary_cache_hit",
            &format!("{} test stub at {}", agent.slug(), stub_path.display()),
        );
        return Ok(AgentBinary {
            agent,
            path: stub_path,
            version: None,
        });
    }

    // Metadata cache hit (TTL: 1hr): skip the network resolve. Absence of the
    // preceding `agent_binary_resolve_started` record is what marks this path
    // in the diagnostics run.
    if let Some(cached_release) = read_cached_release_async(paths, agent).await {
        let cached = cached_binary_path(paths, &cached_release);
        return ensure_binary_or_cached_fallback(
            paths,
            agent,
            &cached_release,
            &cached,
            "cached release download failed",
        )
        .await
        .with_context(|| format!("preparing cached {} binary", agent.slug()));
    }

    if let Some((_, fallback_release, fallback_path)) =
        newest_cached_executable_release_async(paths, agent).await
    {
        spawn_release_metadata_refresh(paths.clone(), agent);
        return ensure_binary_for_release(agent, &fallback_release, &fallback_path).await;
    }

    record(
        "agent_binary_resolve_started",
        &format!("{} latest release", agent.slug()),
    );
    let release = match resolve_latest_release(agent).await {
        Ok(release) => release,
        Err(error) => {
            if let Some((fallback_release, fallback_path)) = cached_executable_after_failure_async(
                paths,
                agent,
                &error,
                "latest version lookup failed",
            )
            .await
            {
                return ensure_binary_for_release(agent, &fallback_release, &fallback_path).await;
            }
            record(
                "agent_binary_failed",
                &format!("{} resolve failed: {error:#}", agent.slug()),
            );
            return Err(error).with_context(|| format!("resolving latest {} binary", agent.slug()));
        }
    };
    record(
        "agent_binary_resolved",
        &format!("{} {} from {}", agent.slug(), release.version, release.url),
    );
    persist_release_cache_async(paths, &release).await;
    let cached = cached_binary_path(paths, &release);
    ensure_binary_or_cached_fallback(
        paths,
        agent,
        &release,
        &cached,
        "latest binary download failed",
    )
    .await
}

fn spawn_release_metadata_refresh(paths: JackinPaths, agent: Agent) {
    #[cfg(test)]
    {
        drop((paths, agent));
    }
    #[cfg(not(test))]
    tokio::spawn(async move {
        record(
            "agent_binary_resolve_started",
            &format!("{} latest release background", agent.slug()),
        );
        match resolve_latest_release(agent).await {
            Ok(release) => {
                record(
                    "agent_binary_resolved",
                    &format!(
                        "{} {} from {} background",
                        agent.slug(),
                        release.version,
                        release.url
                    ),
                );
                persist_release_cache_async(&paths, &release).await;
            }
            Err(error) => {
                record(
                    "warning",
                    &format!(
                        "{} background latest version lookup failed: {error:#}",
                        agent.slug()
                    ),
                );
            }
        }
    });
}

/// Build `release` into `cached`; on failure, fall back to the newest cached
/// executable for `agent` when one exists, else surface the original error.
/// `failure` labels the primary failure in the diagnostics warning.
async fn ensure_binary_or_cached_fallback(
    paths: &JackinPaths,
    agent: Agent,
    release: &AgentRelease,
    cached: &Path,
    failure: &str,
) -> Result<AgentBinary> {
    match ensure_binary_for_release(agent, release, cached).await {
        Ok(binary) => Ok(binary),
        Err(error) => {
            if let Some((fallback_release, fallback_path)) =
                cached_executable_after_failure_async(paths, agent, &error, failure).await
            {
                return ensure_binary_for_release(agent, &fallback_release, &fallback_path).await;
            }
            Err(error)
        }
    }
}

async fn cached_executable_after_failure_async(
    paths: &JackinPaths,
    agent: Agent,
    error: &anyhow::Error,
    failure: &str,
) -> Option<(AgentRelease, PathBuf)> {
    let (_, fallback_release, fallback_path) =
        newest_cached_executable_release_async(paths, agent).await?;
    record(
        "warning",
        &format!(
            "{} {failure}; using cached {} binary at {}: {error:#}",
            agent.slug(),
            fallback_release.version,
            fallback_path.display()
        ),
    );
    Some((fallback_release, fallback_path))
}

/// Return the cached binary if present, otherwise download `release` to
/// `cached` and return it. Shared by the metadata-cache-hit and post-resolve
/// paths so both emit the same breadcrumbs and run the same download sequence.
async fn ensure_binary_for_release(
    agent: Agent,
    release: &AgentRelease,
    cached: &Path,
) -> Result<AgentBinary> {
    if is_executable_file_async(cached).await {
        record(
            "agent_binary_cache_hit",
            &format!(
                "{} {} at {}",
                agent.slug(),
                release.version,
                cached.display()
            ),
        );
        return Ok(AgentBinary {
            agent,
            path: cached.to_path_buf(),
            version: Some(release.version.clone()),
        });
    }
    if repair_cached_binary_mode_async(cached).await? {
        record(
            "agent_binary_cache_repaired",
            &format!(
                "{} {} at {}",
                agent.slug(),
                release.version,
                cached.display()
            ),
        );
        return Ok(AgentBinary {
            agent,
            path: cached.to_path_buf(),
            version: Some(release.version.clone()),
        });
    }
    record(
        "agent_binary_download_started",
        &format!(
            "{} {} from {} to {}",
            agent.slug(),
            release.version,
            release.url,
            cached.display()
        ),
    );
    download_and_cache(release, cached)
        .await
        .with_context(|| {
            format!(
                "downloading {} {} from {}",
                agent.slug(),
                release.version,
                release.url
            )
        })
        .inspect_err(|error| {
            record(
                "agent_binary_failed",
                &format!("{} download failed: {error:#}", agent.slug()),
            );
        })?;
    record(
        "agent_binary_ready",
        &format!(
            "{} {} at {}",
            agent.slug(),
            release.version,
            cached.display()
        ),
    );
    Ok(AgentBinary {
        agent,
        path: cached.to_path_buf(),
        version: Some(release.version.clone()),
    })
}

pub fn cached_binary_path(paths: &JackinPaths, release: &AgentRelease) -> PathBuf {
    paths
        .cache_dir
        .join("agent-binaries")
        .join(release.agent.slug())
        .join(release.version.replace('+', "_"))
        .join(format!("linux-{}", container_arch()))
        .join(release.agent.slug())
}

async fn resolve_latest_release(agent: Agent) -> Result<AgentRelease> {
    // Central resolver exception: each upstream publishes releases through a
    // different mechanism, so this match is the single dispatch point instead
    // of scattering agent-specific release lookup across launch/image code.
    match agent {
        Agent::Claude => resolve_claude().await,
        Agent::Codex => resolve_codex().await,
        Agent::Amp => resolve_amp().await,
        Agent::Kimi => resolve_kimi().await,
        Agent::Opencode => resolve_opencode().await,
        Agent::Grok => resolve_grok().await,
    }
}

async fn resolve_claude() -> Result<AgentRelease> {
    let base = "https://downloads.claude.ai/claude-code-releases";
    let version = fetch_text_with_retry(&format!("{base}/latest")).await?;
    let version = version.trim().to_owned();
    let platform = platform_x64_arm64();
    let manifest: ClaudeManifest = serde_json::from_str(
        &fetch_text_with_retry(&format!("{base}/{version}/manifest.json")).await?,
    )?;
    let entry = manifest
        .platforms
        .get(platform)
        .with_context(|| format!("Claude manifest missing platform {platform}"))?;
    Ok(AgentRelease {
        agent: Agent::Claude,
        version: version.clone(),
        url: format!("{base}/{version}/{platform}/{}", entry.binary),
        checksum: Some(entry.checksum.clone()),
        archive_member: None,
    })
}

async fn resolve_amp() -> Result<AgentRelease> {
    let base = "https://static.ampcode.com";
    let version = fetch_text_with_retry(&format!("{base}/cli/cli-version.txt"))
        .await?
        .trim()
        .to_owned();
    let platform = match container_arch() {
        "arm64" => "linux-arm64",
        _ => "linux-x64",
    };
    let sha_text =
        fetch_text_with_retry(&format!("{base}/cli/{version}/{platform}-amp.sha256")).await?;
    let checksum = parse_sha256_hex(&sha_text)
        .with_context(|| format!("amp published checksum for {version} {platform}"))?;
    Ok(AgentRelease {
        agent: Agent::Amp,
        version: version.clone(),
        url: format!("{base}/cli/{version}/amp-{platform}"),
        checksum: Some(checksum),
        archive_member: None,
    })
}

async fn resolve_kimi() -> Result<AgentRelease> {
    let version = fetch_text_with_retry(&format!("{KIMI_DOWNLOAD_BASE_URL}/latest"))
        .await?
        .trim()
        .to_owned();
    let platform = platform_x64_arm64();
    // URL layout extracted from the official installer at
    // https://code.kimi.com/kimi-code/install.sh:
    //   latest pointer: ${KIMI_DOWNLOAD_BASE}/latest
    //   manifest:       ${KIMI_BINARY_BASE}/${version}/manifest.json
    //   binary:         ${KIMI_BINARY_BASE}/${version}/${filename}
    let manifest: KimiManifest = serde_json::from_str(
        &fetch_text_with_retry(&format!("{KIMI_BINARY_BASE_URL}/{version}/manifest.json")).await?,
    )?;
    let entry = manifest
        .platforms
        .get(platform)
        .with_context(|| format!("Kimi manifest missing platform {platform}"))?;
    Ok(AgentRelease {
        agent: Agent::Kimi,
        version: version.clone(),
        url: format!("{KIMI_BINARY_BASE_URL}/{version}/{}", entry.filename),
        checksum: Some(entry.checksum.clone()),
        archive_member: None,
    })
}

async fn resolve_codex() -> Result<AgentRelease> {
    let arch = match container_arch() {
        "arm64" => "aarch64-unknown-linux-musl",
        _ => "x86_64-unknown-linux-musl",
    };
    let asset = format!("codex-{arch}.tar.gz");
    let release = github_latest_asset("openai/codex", &asset).await?;
    let checksum = release.asset.sha256_digest();
    Ok(AgentRelease {
        agent: Agent::Codex,
        version: release
            .tag_name
            .trim_start_matches("rust-v")
            .trim_start_matches('v')
            .to_owned(),
        url: release.asset.browser_download_url,
        checksum,
        archive_member: Some(format!("codex-{arch}")),
    })
}

async fn resolve_grok() -> Result<AgentRelease> {
    // Version pointer and binary layout extracted from the official installer
    // https://x.ai/cli/install.sh :
    //
    // - Channel pointer (plain text): ${BASE}/stable  (or alpha/enterprise via
    //   GROK_CHANNEL, but we pre-bake "stable" for role images).
    // - Primary base: https://x.ai/cli
    // - Fallback base: https://storage.googleapis.com/grok-build-public-artifacts/cli
    //   (selected if primary probe of the channel pointer fails).
    // - Linux artifact (direct executable, no tarball):
    //     ${BASE}/grok-${VERSION}-linux-x86_64
    //     ${BASE}/grok-${VERSION}-linux-aarch64
    // - No published per-artifact SHA-256 sidecar (unlike Claude/Kimi/Amp or
    //   GitHub digests for Codex/OpenCode). We verify by running `--version`
    //   after download (matching the non-Windows path in the installer).
    // - The binary is also made available as "agent" via symlink by the
    //   install_block in the derived image.
    //
    // Platform/arch mapping matches the installer (os-arch with os=linux).
    let primary = GROK_BASE_PRIMARY;
    let fallback = GROK_BASE_FALLBACK;

    let (base, version) =
        if let Ok(text) = fetch_text_with_retry(&format!("{primary}/stable")).await {
            let v = text.trim().to_owned();
            if v.is_empty() {
                let v = fetch_text_with_retry(&format!("{fallback}/stable"))
                    .await?
                    .trim()
                    .to_owned();
                (fallback.to_owned(), v)
            } else {
                (primary.to_owned(), v)
            }
        } else {
            let v = fetch_text_with_retry(&format!("{fallback}/stable"))
                .await?
                .trim()
                .to_owned();
            (fallback.to_owned(), v)
        };

    if version.is_empty() {
        return Err(ImageError::msg(format!(
            "failed to fetch Grok version pointer from {base}/stable"
        ))
        .into());
    }

    let grok_arch = match container_arch() {
        "arm64" => "aarch64",
        _ => "x86_64",
    };
    let platform = format!("linux-{grok_arch}");
    let url = format!("{base}/grok-{version}-{platform}");

    Ok(AgentRelease {
        agent: Agent::Grok,
        version,
        url,
        checksum: None,
        archive_member: None,
    })
}

async fn resolve_opencode() -> Result<AgentRelease> {
    let arch = match container_arch() {
        "arm64" => "arm64",
        _ => "x64",
    };
    let asset = format!("opencode-linux-{arch}.tar.gz");
    let release = github_latest_asset("anomalyco/opencode", &asset).await?;
    let checksum = release.asset.sha256_digest();
    Ok(AgentRelease {
        agent: Agent::Opencode,
        version: release.tag_name.trim_start_matches('v').to_owned(),
        url: release.asset.browser_download_url,
        checksum,
        archive_member: Some("opencode".to_owned()),
    })
}

async fn fetch_text(url: &str) -> Result<String> {
    record("agent_binary_http_get", url);
    jackin_docker::net::fetch_text(url).await
}

/// Returns true when `error` is a TCP connect-phase timeout. Unlike a slow
/// response or a server-side error, a connect timeout means the endpoint is
/// unreachable and retrying immediately will not help.
fn is_connect_timeout(error: &anyhow::Error) -> bool {
    error.chain().any(|e| {
        e.downcast_ref::<reqwest::Error>()
            .is_some_and(|re| re.is_connect() && re.is_timeout())
    })
}

/// Like `fetch_text` but retries transient HTTP/network errors up to 2 times
/// with exponential back-off. Connect timeouts are not retried: on a warm cache,
/// this lets the caller fall back immediately to the cached executable.
async fn fetch_text_with_retry(url: &str) -> Result<String> {
    let url = url.to_owned();
    retry_metadata_with_backoff(2, Duration::from_millis(500), || {
        let url = url.clone();
        async move { fetch_text(&url).await }
    })
    .await
}

async fn github_auth_token() -> Option<String> {
    GITHUB_AUTH_TOKEN
        .get_or_init(github_auth_token_uncached)
        .await
        .clone()
}

async fn github_auth_token_uncached() -> Option<String> {
    // Degrade to unauthenticated (60 req/hr) on any failure, but log which one:
    // `gh` missing is expected on CI, while present-but-erroring (not logged in)
    // is the case an operator hitting a rate-limit 403 needs to see in --debug.
    match tokio::process::Command::new("gh")
        .args(["auth", "token", "--hostname", "github.com"])
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            let token = String::from_utf8(output.stdout).ok()?.trim().to_owned();
            (!token.is_empty()).then_some(token)
        }
        Ok(output) => {
            jackin_diagnostics::debug_log!(
                "agent_binary",
                "gh auth token exited {}: {} — proceeding unauthenticated",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
            None
        }
        Err(e) => {
            jackin_diagnostics::debug_log!(
                "agent_binary",
                "gh auth token not runnable ({e}) — proceeding unauthenticated"
            );
            None
        }
    }
}

async fn github_latest_asset(repo: &str, asset_name: &str) -> Result<GithubReleaseAssetMatch> {
    let api_url = format!("https://api.github.com/repos/{repo}/releases/latest");
    // Authenticated requests have 5 000 req/hr vs 60 req/hr unauthenticated.
    let token = github_auth_token().await;
    let mut headers = HeaderMap::new();
    if let Some(ref t) = token {
        let val = reqwest::header::HeaderValue::from_str(&format!("Bearer {t}"))
            .context("building Authorization header from gh token")?;
        headers.insert(reqwest::header::AUTHORIZATION, val);
    }
    let client = jackin_docker::net::http_client(headers)?;
    let body = retry_metadata_with_backoff(2, Duration::from_millis(500), || {
        let c = client.clone();
        let u = api_url.clone();
        async move {
            record("agent_binary_http_get", &u);
            jackin_docker::net::get_text(&c, &u).await
        }
    })
    .await
    .with_context(|| format!("fetching latest GitHub release metadata for {repo}"))?;
    let release: GithubRelease = serde_json::from_str(&body)
        .with_context(|| format!("parsing latest GitHub release metadata for {repo}"))?;
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == asset_name)
        .with_context(|| format!("{repo} latest release missing asset {asset_name}"))?;
    Ok(GithubReleaseAssetMatch {
        tag_name: release.tag_name,
        asset,
    })
}

async fn retry_metadata_with_backoff<T, F, Fut>(
    max_attempts: u32,
    initial_delay: Duration,
    f: F,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut last_err = ImageError::NoAttemptsMade.into();
    for attempt in 0..max_attempts {
        if attempt > 0 {
            let delay = initial_delay * (1 << (attempt - 1));
            record(
                "retry_backoff",
                &format!("attempt {attempt}/{max_attempts}, waiting {delay:?}"),
            );
            tokio::time::sleep(delay).await;
        }
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if is_connect_timeout(&e) => return Err(e),
            Err(e) => {
                record(
                    "retry_failed",
                    &format!("attempt {}/{max_attempts}: {e:#}", attempt + 1),
                );
                last_err = e;
            }
        }
    }
    Err(last_err).with_context(|| format!("giving up after {max_attempts} attempts"))
}

async fn retry_with_backoff<T, F, Fut>(
    max_attempts: u32,
    initial_delay: Duration,
    f: F,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut last_err = ImageError::NoAttemptsMade.into();
    for attempt in 0..max_attempts {
        if attempt > 0 {
            let delay = initial_delay * (1 << (attempt - 1));
            record(
                "retry_backoff",
                &format!("attempt {attempt}/{max_attempts}, waiting {delay:?}"),
            );
            tokio::time::sleep(delay).await;
        }
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                record(
                    "retry_failed",
                    &format!("attempt {}/{max_attempts}: {e:#}", attempt + 1),
                );
                last_err = e;
            }
        }
    }
    Err(last_err).with_context(|| format!("giving up after {max_attempts} attempts"))
}

fn platform_x64_arm64() -> &'static str {
    match container_arch() {
        "arm64" => "linux-arm64",
        _ => "linux-x64",
    }
}

async fn download_and_cache(release: &AgentRelease, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp_download = dest.with_extension("download.tmp");
    let tmp_binary = dest.with_extension("tmp");
    drop(tokio::fs::remove_file(&tmp_download).await);
    drop(tokio::fs::remove_file(&tmp_binary).await);
    let result = download_and_cache_inner(release, dest, &tmp_download, &tmp_binary).await;
    if result.is_err() {
        drop(tokio::fs::remove_file(&tmp_download).await);
        drop(tokio::fs::remove_file(&tmp_binary).await);
    }
    result
}

async fn download_and_cache_inner(
    release: &AgentRelease,
    dest: &Path,
    tmp_download: &Path,
    tmp_binary: &Path,
) -> Result<()> {
    retry_with_backoff(3, Duration::from_millis(500), || {
        let url = release.url.clone();
        let tmp_download = tmp_download.to_owned();
        async move { jackin_docker::net::download_parallel(&url, &tmp_download).await }
    })
    .await
    .with_context(|| format!("downloading {}", release.url))?;
    // A dropped chunk leaves a zeroed hole in the pre-sized file rather than a
    // short file, so the SHA-256 (when published) is the integrity guard.
    //
    // Claude/Kimi/Amp publish checksums in their manifests.
    // Codex/OpenCode get them from GitHub release asset digests.
    //
    // Grok (per analysis of https://x.ai/cli/install.sh) does not publish a
    // per-artifact SHA sidecar for the direct linux binary. We fall back to a
    // `--version` smoke test after download (exactly as the official installer
    // does on non-Windows) to verify we got a runnable binary for that version.
    if let Some(expected) = release.checksum.as_deref() {
        let tmp_for_hash = tmp_download.to_owned();
        let actual = tokio::task::spawn_blocking(move || hash_file_sha256(&tmp_for_hash))
            .await
            .context("hash worker join")?
            .with_context(|| format!("hashing {}", tmp_download.display()))?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(ImageError::msg(format!(
                "{} checksum mismatch for {}\n  expected {}\n  actual   {}",
                release.agent.slug(),
                release.url,
                expected,
                actual
            ))
            .into());
        }
    } else if release.agent != Agent::Grok {
        // Future agents without checksums should be explicitly handled (or
        // provide one). Only Grok is currently allowed to skip SHA.
        return Err(ImageError::msg(format!(
            "{} release {} has no published checksum; refusing to install an unverified binary",
            release.agent.slug(),
            release.version
        ))
        .into());
    }
    if let Some(member) = &release.archive_member {
        let archive = tmp_download.to_owned();
        let member = member.clone();
        let output = tmp_binary.to_owned();
        tokio::task::spawn_blocking(move || extract_tar_gz_member(&archive, &member, &output))
            .await
            .context("archive extraction worker join")??;
        drop(tokio::fs::remove_file(tmp_download).await);
    } else {
        tokio::fs::rename(tmp_download, tmp_binary).await?;
    }
    let binary_for_chmod = tmp_binary.to_owned();
    tokio::task::spawn_blocking(move || chmod_executable(&binary_for_chmod))
        .await
        .context("chmod worker join")??;

    // Smoke test for agents without a published checksum (Grok today).
    // This mirrors the `if ! "$binary_tmp" --version ...` check in the official
    // https://x.ai/cli/install.sh (non-Windows path) and gives us the
    // "verifying the new version" guarantee before we trust the cached binary.
    //
    // Only performed on Linux hosts: the cached artifacts are always Linux
    // binaries (for injection into role containers). On macOS/Windows dev
    // machines we cannot natively exec them; Grok remains the one prefetched
    // install block that keeps a Docker-build `grok --version` smoke check.
    if release.checksum.is_none() && cfg!(target_os = "linux") {
        let status = tokio::process::Command::new(tmp_binary)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .with_context(|| {
                format!(
                    "spawning {} --version smoke test for downloaded binary",
                    release.agent.slug()
                )
            })?;
        if !status.success() {
            return Err(ImageError::msg(format!(
                "{} {} failed --version smoke test after download (status: {:?})",
                release.agent.slug(),
                release.version,
                status
            ))
            .into());
        }
    }

    tokio::fs::rename(tmp_binary, dest).await?;
    Ok(())
}

fn record(kind: &str, message: &str) {
    if let Some(run) = jackin_diagnostics::active_run() {
        run.compact(kind, message);
    } else {
        jackin_diagnostics::debug_log!("agent_binary", "{kind}: {message}");
    }
}

fn test_stub_path(paths: &JackinPaths, agent: Agent) -> PathBuf {
    paths
        .cache_dir
        .join("agent-binaries-test-stub")
        .join(agent.slug())
}

pub fn install_test_stub(paths: &JackinPaths, agent: Agent) -> Result<()> {
    let stub = test_stub_path(paths, agent);
    if let Some(parent) = stub.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&stub, b"#!/bin/sh\nprintf 'stub agent binary\\n'\n")?;
    chmod_executable(&stub)?;
    Ok(())
}

fn metadata_cache_path(paths: &JackinPaths, agent: Agent) -> PathBuf {
    paths
        .cache_dir
        .join("agent-binaries")
        .join(agent.slug())
        .join("latest.json")
}

fn version_metadata_path(paths: &JackinPaths, release: &AgentRelease) -> PathBuf {
    cached_binary_path(paths, release).with_file_name("metadata.json")
}

fn read_release_file(path: &Path) -> Option<AgentRelease> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn read_cached_release(paths: &JackinPaths, agent: Agent) -> Option<AgentRelease> {
    read_cached_release_with_clock(paths, agent, &SystemClock)
}

fn read_cached_release_with_clock(
    paths: &JackinPaths,
    agent: Agent,
    clock: &dyn Clock,
) -> Option<AgentRelease> {
    read_cached_release_at(paths, agent, clock.now_system())
}

/// TTL check against an injected wall-clock instant (plan 025).
fn read_cached_release_at(
    paths: &JackinPaths,
    agent: Agent,
    now: SystemTime,
) -> Option<AgentRelease> {
    let path = metadata_cache_path(paths, agent);
    let metadata = std::fs::metadata(&path).ok()?;
    let modified = metadata.modified().ok()?;
    if now.duration_since(modified).ok()? >= CACHE_TTL {
        return None;
    }
    read_release_file(&path)
}

async fn read_cached_release_async(paths: &JackinPaths, agent: Agent) -> Option<AgentRelease> {
    let paths = paths.clone();
    match tokio::task::spawn_blocking(move || read_cached_release(&paths, agent)).await {
        Ok(release) => release,
        Err(error) => {
            // A join error here means the worker was cancelled or panicked; the
            // read itself is panic-free today, so report it rather than letting a
            // future panicking read masquerade as a cache miss.
            jackin_diagnostics::debug_log!(
                "agent_binary",
                "cache read worker failed for {}: {error:#}",
                agent.slug()
            );
            None
        }
    }
}

pub fn newest_cached_executable_release(
    paths: &JackinPaths,
    agent: Agent,
) -> Option<(SystemTime, AgentRelease, PathBuf)> {
    let root = paths.cache_dir.join("agent-binaries").join(agent.slug());
    let arch_dir_name = format!("linux-{}", container_arch());
    let mut candidates = Vec::new();
    for version_entry in std::fs::read_dir(root).ok()?.flatten() {
        let metadata_path = version_entry
            .path()
            .join(&arch_dir_name)
            .join("metadata.json");
        let Some(release) = read_release_file(&metadata_path).filter(|release| {
            release.agent == agent && is_executable_file(&cached_binary_path(paths, release))
        }) else {
            continue;
        };
        let binary_path = cached_binary_path(paths, &release);
        let modified = std::fs::metadata(&binary_path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        candidates.push((modified, release, binary_path));
    }
    candidates.sort_by_key(|(modified, _, _)| std::cmp::Reverse(*modified));
    candidates.into_iter().next()
}

async fn newest_cached_executable_release_async(
    paths: &JackinPaths,
    agent: Agent,
) -> Option<(SystemTime, AgentRelease, PathBuf)> {
    let paths = paths.clone();
    match tokio::task::spawn_blocking(move || newest_cached_executable_release(&paths, agent)).await
    {
        Ok(found) => found,
        Err(error) => {
            jackin_diagnostics::debug_log!(
                "agent_binary",
                "cache scan worker failed for {}: {error:#}",
                agent.slug()
            );
            None
        }
    }
}

fn write_cached_release(paths: &JackinPaths, release: &AgentRelease) -> Result<()> {
    let path = metadata_cache_path(paths, release.agent);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string(release)?)?;
    Ok(())
}

fn write_version_release(paths: &JackinPaths, release: &AgentRelease) -> Result<()> {
    let path = version_metadata_path(paths, release);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(release)?)?;
    Ok(())
}

/// Best-effort write of the resolved release to the TTL metadata cache and the
/// per-version sidecar. A failure only costs an extra network resolve next
/// launch, so it's logged (to explain a re-resolve loop under `--debug`) rather
/// than propagated.
async fn persist_release_cache_async(paths: &JackinPaths, release: &AgentRelease) {
    let paths = paths.clone();
    let release = release.clone();
    let slug = release.agent.slug();
    let result = tokio::task::spawn_blocking(move || {
        (
            write_cached_release(&paths, &release),
            write_version_release(&paths, &release),
        )
    })
    .await;
    match result {
        Ok((cached, version)) => {
            if let Err(e) = cached {
                jackin_diagnostics::debug_log!(
                    "agent_binary",
                    "caching {slug} release metadata failed: {e:#}"
                );
            }
            if let Err(e) = version {
                jackin_diagnostics::debug_log!(
                    "agent_binary",
                    "writing {slug} version sidecar failed: {e:#}"
                );
            }
        }
        Err(e) => jackin_diagnostics::debug_log!(
            "agent_binary",
            "cache metadata worker failed for {slug}: {e:#}"
        ),
    }
}

async fn is_executable_file_async(path: &Path) -> bool {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || is_executable_file(&path))
        .await
        .unwrap_or(false)
}

async fn repair_cached_binary_mode_async(path: &Path) -> Result<bool> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || repair_executable_file(&path))
        .await
        .context("cache chmod worker join")?
}

#[derive(Debug, Deserialize)]
struct ClaudeManifest {
    platforms: std::collections::HashMap<String, ClaudePlatform>,
}

#[derive(Debug, Deserialize)]
struct ClaudePlatform {
    binary: String,
    checksum: String,
}

#[derive(Debug, Deserialize)]
struct KimiManifest {
    platforms: std::collections::HashMap<String, KimiPlatform>,
}

#[derive(Debug, Deserialize)]
struct KimiPlatform {
    filename: String,
    checksum: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

impl GithubAsset {
    fn sha256_digest(&self) -> Option<String> {
        self.digest
            .as_ref()?
            .strip_prefix("sha256:")
            .map(str::to_owned)
    }
}

struct GithubReleaseAssetMatch {
    tag_name: String,
    asset: GithubAsset,
}

#[cfg(test)]
mod tests;
